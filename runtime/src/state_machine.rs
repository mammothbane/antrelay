use actix::prelude::*;
use actix_broker::{
    BrokerSubscribe,
    SystemBroker,
};
use futures::future::BoxFuture;
use std::time::Duration;

use message::{
    header::{
        Destination,
        Event,
    },
    payload::Ack,
    BytesWrap,
    Message,
};

use crate::{
    ground,
    params,
    serial,
};

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, derive_more::Display)]
#[repr(u8)]
pub enum State {
    FlightIdle,
    PingCentralStation,
    StartBLE,
    PollAnt,
    AntRun,
}

pub struct StateMachine {
    state:        State,
    running_task: Option<SpawnHandle>,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self {
            state:        State::FlightIdle,
            running_task: None,
        }
    }
}

async fn send_retry(
    msg: impl FnMut() -> BoxFuture<'static, message::Message<BytesWrap>>,
) -> Result<Message<Ack>, serial::Error> {
    // proportion of signal to be jittered, i.e. multiply the signal by a random sample in the range
    // [1 - JITTER_FACTOR, 1 + JITTER_FACTOR]
    const JITTER_FACTOR: f64 = 0.5;

    let strategy = tokio_retry::strategy::ExponentialBackoff::from_millis(100)
        .max_delay(Duration::from_secs(2))
        .map(|dur| {
            // make distribution even about 0, scale by factor, offset about 1
            let jitter = (rand::random::<f64>() - 0.5) * JITTER_FACTOR * 2. + 1.;

            dur.mul_f64(jitter)
        })
        .take(10);

    serial::send_retry(msg, Duration::from_millis(750), strategy).await
}

async fn ping_cs() {
    let msg = message::command(&params().await, Destination::CentralStation, Event::CSPing);

    serial::do_send(msg).await;
}

async fn ping_ant() {
    loop {
        let result = send_retry(|| {
            Box::pin(
                async move { message::command(&params().await, Destination::Ant, Event::AntPing) },
            )
        })
        .await;

        if let Err(e) = result {
            tracing::error!(error = %e, "pinging ant");
        }
    }
}

impl StateMachine {
    #[tracing::instrument(skip_all, fields(state = ?self.state, event = ?event))]
    fn step(&mut self, event: Event, ctx: &mut Context<Self>) -> Result<(), serial::Error> {
        let mut old_handle = self.running_task.take();

        tracing::debug!("handling state transition");
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
                .map(|result, act: &mut Self, _| {
                    if let Err(e) = result {
                        tracing::error!(error = %e, "failed to send garage open to central station");
                        return;
                    }

                    act.state = State::StartBLE;
                });

                ctx.wait(fut);
            },

            (State::StartBLE, Event::CSGarageOpen) => {
                self.running_task = Some(ctx.spawn(fut::wrap_future(ping_ant())));

                self.state = State::PollAnt;
            },

            (State::PollAnt, Event::FERoverStop) => {
                let fut = fut::wrap_future(send_retry(|| {
                    Box::pin(async move {
                        message::command(&params().await, Destination::Ant, Event::AntCalibrate)
                    })
                }))
                .map(|_result, act: &mut Self, _| {
                    act.state = State::AntRun;
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
        self.subscribe_async::<SystemBroker, ground::UpCommand>(ctx);
        tracing::info!("state machine controller started");
    }
}

impl Handler<ground::UpCommand> for StateMachine {
    type Result = MessageResult<ground::UpCommand>;

    #[tracing::instrument(skip_all, fields(msg = %msg.0))]
    fn handle(&mut self, msg: ground::UpCommand, ctx: &mut Self::Context) -> Self::Result {
        let event = msg.0.as_ref().header.ty.event;

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
