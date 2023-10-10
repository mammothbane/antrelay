use std::{
    sync::Once,
    time::Duration,
};

use actix::prelude::*;
use actix_broker::{
    BrokerSubscribe,
    SystemBroker,
};
use futures::future::BoxFuture;

use message::{
    header::{
        Destination,
        Event,
    },
    BytesWrap,
};

use crate::{
    ground,
    params,
    serial,
    serial::send,
};

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, derive_more::Display)]
#[repr(u8)]
pub enum State {
    FlightIdle,
    PingCentralStation,
    GarageOpen,
    BLEConnected,
    AntReady,
    AntRun,
}

pub struct StateMachine {
    state:          State,
    running_task:   Option<SpawnHandle>,
    subscribe_once: Once,
    pending_evt:    Option<Event>,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self {
            state:          State::FlightIdle,
            running_task:   None,
            subscribe_once: Once::new(),
            pending_evt:    None,
        }
    }
}

#[tracing::instrument(skip(msg))]
async fn send_retry(
    msg: impl FnMut() -> BoxFuture<'static, message::Message<BytesWrap>>,
    timeout: Duration,
    base_retry: Duration,
    max_delay: Duration,
    max_count: usize,
) -> Result<serial::Response, serial::Error> {
    // proportion of signal to be jittered, i.e. multiply the signal by a random sample in the range
    // [1 - JITTER_FACTOR, 1 + JITTER_FACTOR]
    const JITTER_FACTOR: f64 = 0.5;

    let strategy =
        tokio_retry::strategy::ExponentialBackoff::from_millis(base_retry.as_millis() as u64)
            .max_delay(max_delay)
            .map(|dur| {
                // make distribution even about 0, scale by factor, offset about 1
                let jitter = (rand::random::<f64>() - 0.5) * JITTER_FACTOR * 2. + 1.;

                dur.mul_f64(jitter)
            })
            .take(max_count);

    serial::send_retry(msg, timeout, strategy).await
}

async fn ping_cs() {
    let msg = message::command(&params().await, Destination::CentralStation, Event::CSPing);

    serial::do_send(msg).await;
}

async fn send_ant_start() {
    let msg = message::command(&params().await, Destination::Ant, Event::AntStart);

    if let Err(e) = send(msg, Some(Duration::SECOND * 30)).await {
        tracing::error!(error = %e, "sending ant start");
    }
}

impl StateMachine {
    #[tracing::instrument(skip_all, fields(state = ?self.state, event = ?event))]
    fn step(&mut self, event: Event, ctx: &mut Context<Self>) -> Result<(), serial::Error> {
        let mut old_handle = self.running_task.take();

        tracing::info!("state machine event");
        let init_state = self.state;

        let mut reset_handle = || {
            self.running_task = old_handle;
            old_handle = None;
        };

        let mut ignore_repeat = || {
            reset_handle();
            tracing::info!("noop event: already in target state");
        };

        match (init_state, event) {
            (State::PingCentralStation, Event::FEPowerSupplied) => ignore_repeat(),
            (_, Event::FEPowerSupplied) => {
                let handle = ctx.run_interval(Duration::from_secs(5), move |_a, ctx| {
                    ctx.spawn(fut::wrap_future(ping_cs()));
                });

                self.running_task = Some(handle);
                self.state = State::PingCentralStation;
            },

            (State::GarageOpen, Event::FEGarageOpen) => ignore_repeat(),
            (_, Event::FEGarageOpen) => {
                let fut = fut::wrap_future(send_retry(
                    || {
                        Box::pin(async move {
                            message::command(
                                &params().await,
                                Destination::CentralStation,
                                Event::CSGarageOpen,
                            )
                        })
                    },
                    Duration::from_secs(3),
                    Duration::SECOND,
                    Duration::SECOND * 5,
                    3,
                ))
                .map(|result, act: &mut Self, _ctx| {
                    if let Err(e) = result {
                        tracing::error!(error = %e, "sending garage open to central station");
                        return;
                    }

                    act.running_task = None;
                    act.state = State::GarageOpen;

                    tracing::info!("transitioned to garage open state");
                });

                self.running_task = Some(ctx.spawn(fut));
            },

            (_, Event::CSBLEDisconnect) => self.state = State::GarageOpen,

            (State::BLEConnected, Event::CSBLEConnect) => ignore_repeat(),
            (_, Event::CSBLEConnect) => {
                let handle = ctx.run_later(Duration::SECOND * 60, |a, ctx| {
                    a.state = State::AntReady;

                    let Some(evt) = a.pending_evt.take() else {
                        return;
                    };

                    tracing::info!("sending queued rover stop event");

                    ctx.address().do_send(EventWrap(evt));
                });

                self.state = State::BLEConnected;
                self.running_task = Some(handle);
            },

            (State::AntRun, Event::FERoverStop) => ignore_repeat(),
            (State::AntReady, Event::FERoverStop) => {
                let handle = ctx.run_later(Duration::SECOND * 10, |a, ctx| {
                    ctx.wait(fut::wrap_future(send_ant_start()));

                    let handle = ctx.run_interval(Duration::SECOND * 30, |_a, ctx| {
                        ctx.wait(fut::wrap_future(send_ant_start()));
                    });

                    a.running_task = Some(handle);
                });

                self.running_task = Some(handle);
                self.state = State::AntRun;
            },

            (State::AntRun, Event::FERoverMove) => {
                let fut = fut::wrap_future(async move {
                    if let Err(e) = send_retry(
                        || {
                            Box::pin(async move {
                                message::command(&params().await, Destination::Ant, Event::AntStop)
                            })
                        },
                        Duration::from_secs(20),
                        Duration::from_secs(5),
                        Duration::from_secs(10),
                        3,
                    )
                    .await
                    {
                        tracing::error!(error = %e, "sending ant stop");
                    }
                });

                self.running_task = Some(ctx.spawn(fut));
                self.state = State::AntReady;
            },

            (_, Event::FERoverStop) => {
                tracing::info!("queueing rover stop for later");
                self.pending_evt = Some(Event::FERoverStop);
                reset_handle();
            },
            (_, Event::FERoverMove) => {
                tracing::info!("clearing pending event");
                self.pending_evt = None;
                reset_handle();
            },

            // below: non-state-affecting commands
            (_, Event::FERestart) => {
                tracing::warn!("received restart, exiting");
                std::process::exit(1);
            },

            #[cfg(debug_assertions)]
            (_, Event::DebugCSPing) => {
                let fut = fut::wrap_future(async move {
                    let msg = message::command(
                        &params().await,
                        Destination::CentralStation,
                        Event::CSPing,
                    );

                    serial::do_send(msg).await;
                });

                reset_handle();
                ctx.wait(fut);
            },

            (_, Event::AntPing) => {
                let fut = fut::wrap_future(async move {
                    let msg = message::command(&params().await, Destination::Ant, Event::AntPing);

                    serial::do_send(msg).await;
                });

                reset_handle();
                ctx.wait(fut);
            },

            (_, Event::FEPing) => {
                tracing::info!("pong");
                reset_handle();
            },

            (ref state, event) => {
                tracing::debug!(?state, ?event, "unmatched state machine transition");
                reset_handle();
            },
        };

        if let Some(handle) = old_handle {
            ctx.cancel_future(handle);
        }

        if self.state != init_state {
            tracing::info!(new_state = ?self.state, "state machine transition");
        } else {
            tracing::info!("state machine did not transition");
        }

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    fn do_step(&mut self, event: Event, ctx: &mut Context<Self>) {
        if let Err(e) = self.step(event, ctx) {
            tracing::error!(error = %e, "advancing state machine");
        }
    }
}

impl Actor for StateMachine {
    type Context = Context<Self>;

    #[tracing::instrument(skip_all)]
    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_once.call_once(|| {
            self.subscribe_async::<SystemBroker, ground::UpCommand>(ctx);
            self.subscribe_async::<SystemBroker, serial::DownMessage>(ctx);
        });

        tracing::info!("state machine controller started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::error!("state machine controller stopped");
    }
}

impl Handler<ground::UpCommand> for StateMachine {
    type Result = MessageResult<ground::UpCommand>;

    #[tracing::instrument(skip_all, fields(%msg))]
    fn handle(
        &mut self,
        ground::UpCommand(msg): ground::UpCommand,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let hdr = msg.header.header;
        self.do_step(hdr.ty.event, ctx);

        // forward any messages for the ant or the cs from the ground
        if matches!(hdr.destination, Destination::Ant | Destination::CentralStation) {
            ctx.spawn(fut::wrap_future(serial::do_send(msg)));
        }

        MessageResult(())
    }
}

impl Handler<serial::DownMessage> for StateMachine {
    type Result = MessageResult<serial::DownMessage>;

    #[inline]
    #[tracing::instrument(skip_all)]
    fn handle(
        &mut self,
        serial::DownMessage(msg): serial::DownMessage,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let evt = msg.header.header.ty.event;

        if !matches!(evt, Event::CSBLEConnect | Event::CSBLEDisconnect) {
            tracing::trace!("state machine ignoring non-BLE event");
            return MessageResult(());
        }

        self.do_step(evt, ctx);

        MessageResult(())
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct EventWrap(Event);

impl Handler<EventWrap> for StateMachine {
    type Result = ();

    #[inline]
    #[tracing::instrument(skip_all)]
    fn handle(&mut self, EventWrap(event): EventWrap, ctx: &mut Self::Context) -> Self::Result {
        self.do_step(event, ctx);
    }
}

impl Supervised for StateMachine {
    fn restarting(&mut self, _ctx: &mut <Self as Actor>::Context) {
        tracing::warn!("state machine controller restarting");
    }
}
