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
    PollAnt,
    AntRun,
}

pub struct StateMachine {
    state:          State,
    running_task:   Option<SpawnHandle>,
    subscribe_once: Once,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self {
            state:          State::FlightIdle,
            running_task:   None,
            subscribe_once: Once::new(),
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
) -> Result<crate::serial::Response, serial::Error> {
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
                let handle = ctx.run_later(Duration::from_secs(5), move |a, ctx| {
                    let handle = ctx.run_interval(Duration::from_secs(5), move |_a, ctx| {
                        ctx.spawn(fut::wrap_future(ping_cs()));
                    });

                    a.running_task = Some(handle);
                });

                self.running_task = Some(handle);
                self.state = State::PingCentralStation;
            },

            (State::PollAnt, Event::FEGarageOpen) => ignore_repeat(),
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
                    act.state = State::PollAnt;
                });

                self.running_task = Some(ctx.spawn(fut));
            },

            (State::AntRun, Event::FERoverStop) => ignore_repeat(),
            (_, Event::FERoverStop) => {
                let fut = fut::wrap_future(async move {
                    let msg = message::command(&params().await, Destination::Ant, Event::AntStart);

                    if let Err(e) = send(msg, Some(Duration::SECOND * 20)).await {
                        tracing::error!(error = %e, "sending ant start");
                    }
                });

                self.running_task = Some(ctx.spawn(fut));
                self.state = State::AntRun;
            },

            (State::PollAnt, Event::FERoverMove) => ignore_repeat(),
            (_, Event::FERoverMove) => {
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
                self.state = State::PollAnt;
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
}

impl Actor for StateMachine {
    type Context = Context<Self>;

    #[tracing::instrument(skip_all)]
    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_once.call_once(|| {
            self.subscribe_async::<SystemBroker, ground::UpCommand>(ctx);
        });

        tracing::info!("state machine controller started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::error!("state machine controller stopped");
    }
}

impl Handler<ground::UpCommand> for StateMachine {
    type Result = MessageResult<ground::UpCommand>;

    #[tracing::instrument(skip_all, fields(msg = %msg.0))]
    fn handle(&mut self, msg: ground::UpCommand, ctx: &mut Self::Context) -> Self::Result {
        let event = msg.0.header.header.ty.event;

        if let Err(e) = self.step(event, ctx) {
            tracing::error!(error = %e, "advancing state machine");
        }

        MessageResult(())
    }
}

impl Supervised for StateMachine {
    fn restarting(&mut self, _ctx: &mut <Self as Actor>::Context) {
        tracing::warn!("state machine controller restarting");
    }
}
