use std::{
    sync::Once,
    time::Duration,
};

use actix::prelude::*;
use actix_broker::{
    BrokerIssue,
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
    ground::Log,
    params,
    serial,
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

async fn send_retry(
    msg: impl FnMut() -> BoxFuture<'static, message::Message<BytesWrap>>,
) -> Result<message::Message, serial::Error> {
    // proportion of signal to be jittered, i.e. multiply the signal by a random sample in the range
    // [1 - JITTER_FACTOR, 1 + JITTER_FACTOR]
    const JITTER_FACTOR: f64 = 0.5;

    let strategy = tokio_retry::strategy::ExponentialBackoff::from_millis(300)
        .max_delay(Duration::from_secs(3))
        .map(|dur| {
            // make distribution even about 0, scale by factor, offset about 1
            let jitter = (rand::random::<f64>() - 0.5) * JITTER_FACTOR * 2. + 1.;

            dur.mul_f64(jitter)
        })
        .take(5);

    serial::send_retry(msg, Duration::from_millis(750), strategy).await
}

async fn ping_cs() {
    let msg = message::command(&params().await, Destination::CentralStation, Event::CSPing);

    serial::do_send(msg).await;
}

async fn ping_ant() {
    loop {
        let msg = message::command(&params().await, Destination::Ant, Event::AntPing);
        let result = serial::send(msg, Some(Duration::from_millis(750))).await;

        if let Err(e) = result {
            tracing::error!(error = %e, "pinging ant");
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

impl StateMachine {
    #[tracing::instrument(skip_all, fields(state = ?self.state, event = ?event))]
    fn step(&mut self, event: Event, ctx: &mut Context<Self>) -> Result<(), serial::Error> {
        let mut old_handle = self.running_task.take();

        tracing::debug!("handling event");
        let init_state = self.state;

        match (init_state, event) {
            (State::FlightIdle, Event::FEPowerSupplied) => {
                let handle = ctx.run_interval(Duration::from_secs(5), move |_a, ctx| {
                    ctx.spawn(fut::wrap_future(ping_cs()));
                });

                self.running_task = Some(handle);
                self.state = State::PingCentralStation;
            },

            (State::PingCentralStation, Event::FEGarageOpen) => {
                let fut = fut::wrap_future(send_retry(|| {
                    Box::pin(async move {
                        message::command(
                            &params().await,
                            Destination::CentralStation,
                            Event::CSGarageOpen,
                        )
                    })
                }))
                .map(|result, act: &mut Self, _ctx| {
                    if let Err(e) = result {
                        tracing::error!(error = %e, "failed to send garage open to central station");
                        return;
                    }

                    act.running_task = None;
                    act.state = State::PollAnt;
                });

                ctx.wait(fut);
            },

            (State::PollAnt, Event::FERoverStop) => {
                let fut = fut::wrap_future(async move {
                    send_retry(|| {
                        Box::pin(async move {
                            message::command(&params().await, Destination::Ant, Event::AntStart)
                        })
                    })
                    .await?;

                    Ok(()) as Result<(), serial::Error>
                })
                .map(|result, act: &mut Self, _| {
                    if let Err(e) = result {
                        tracing::error!(error = %e, "failed to transition ant to running");
                        return;
                    }

                    act.state = State::AntRun;
                });

                ctx.wait(fut);
            },

            (State::AntRun, Event::FERoverMove) => {
                let fut = fut::wrap_future(async move {
                    send_retry(|| {
                        Box::pin(async move {
                            message::command(&params().await, Destination::Ant, Event::AntStart)
                        })
                    })
                })
                .map(|_result, act: &mut Self, ctx| {
                    act.running_task = Some(ctx.spawn(fut::wrap_future(ping_ant())));
                    act.state = State::PollAnt;
                });

                ctx.wait(fut);
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

                ctx.wait(fut);
            },

            (_, Event::AntPing) => {
                let fut = fut::wrap_future(async move {
                    let msg = message::command(&params().await, Destination::Ant, Event::AntPing);

                    serial::do_send(msg).await;
                });

                ctx.wait(fut);
            },

            (_, Event::FEPing) => {
                self.issue_sync::<SystemBroker, _>(Log("pong".into()), ctx);
            },

            (ref state, event) => {
                tracing::debug!(?state, ?event, "unmatched state machine transition");
                self.running_task = old_handle;
                old_handle = None;
            },
        };

        if let Some(handle) = old_handle {
            ctx.cancel_future(handle);
        }

        if self.state != init_state {
            tracing::info!("state transition: {:?} -> {:?}", init_state, self.state);
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
}

impl Handler<ground::UpCommand> for StateMachine {
    type Result = MessageResult<ground::UpCommand>;

    #[tracing::instrument(skip_all, fields(msg = %msg.0))]
    fn handle(&mut self, msg: ground::UpCommand, ctx: &mut Self::Context) -> Self::Result {
        let event = msg.0.as_ref().header.header.ty.event;

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
