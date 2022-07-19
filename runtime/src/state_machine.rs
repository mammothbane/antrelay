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
        Server,
    },
    payload::Ack,
    BytesWrap,
    Message,
};

use crate::{
    context::ContextExt,
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
    CalibrateIMU,
    AntRun,
}

pub struct StateMachine {
    state:        State,
    running_task: Option<SpawnHandle>,
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
    let msg = message::command(
        &params().await,
        Destination::CentralStation,
        Server::CentralStation,
        Event::CS_PING,
    );

    serial::do_send(msg).await;
}

async fn ping_ant() {
    loop {
        let result = send_retry(|| {
            Box::pin(async move {
                message::command(&params().await, Destination::Ant, Server::Ant, Event::A_PING)
            })
        })
        .await;

        if let Err(e) = result {
            tracing::error!(error = %e, "pinging ant");
        }
    }
}

impl StateMachine {
    fn step(&mut self, event_type: Event, ctx: &mut Context<Self>) -> Result<(), serial::Error> {
        let mut old_handle = self.running_task.take();

        match (self.state, event_type) {
            (State::FlightIdle, Event::FE_5V_SUP) => {
                let handle = ctx.run_interval(Duration::from_secs(5), move |_a, ctx| {
                    ctx.await_(ping_cs());
                });

                self.running_task = Some(handle);
                self.state = State::PingCentralStation;
            },

            (State::PingCentralStation, Event::FE_GARAGE_OPEN) => {
                let fut = send_retry(|| {
                    Box::pin(async move {
                        message::command(
                            &params().await,
                            Destination::CentralStation,
                            Server::CentralStation,
                            Event::CS_GARAGE_OPEN,
                        )
                    })
                });

                ctx.await_(fut)?;

                self.state = State::StartBLE;
            },

            (State::StartBLE, Event::CS_GARAGE_OPEN) => {
                self.running_task = Some(ctx.spawn(fut::wrap_future(ping_ant())));

                self.state = State::PollAnt;
            },

            (State::PollAnt, Event::FE_ROVER_STOP) => {
                ctx.await_(send_retry(|| {
                    Box::pin(async move {
                        message::command(
                            &params().await,
                            Destination::Ant,
                            Server::Ant,
                            Event::A_CALI,
                        )
                    })
                }))?;

                self.state = State::AntRun;
            },

            (State::CalibrateIMU, Event::A_CALI) => {
                self.state = State::AntRun;
            },

            (State::CalibrateIMU | State::AntRun, Event::FE_ROVER_MOVE) => {
                self.state = State::PollAnt;
            },

            #[cfg(debug_assertions)]
            (_, Event::FE_CS_PING) => {
                let msg = message::command(
                    &ctx.await_(params()),
                    Destination::CentralStation,
                    Server::CentralStation,
                    Event::CS_PING,
                );

                ctx.await_(serial::do_send(msg));
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

        Ok(())
    }
}

impl Actor for StateMachine {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_sync::<SystemBroker, ground::UpCommand>(ctx);
    }
}

impl Handler<ground::UpCommand> for StateMachine {
    type Result = MessageResult<ground::UpCommand>;

    fn handle(&mut self, msg: ground::UpCommand, ctx: &mut Self::Context) -> Self::Result {
        let event = msg.0.as_ref().header.ty.event;

        if let Err(e) = self.step(event, ctx) {
            tracing::error!(error = %e, "advancing state machine");
        }

        MessageResult(())
    }
}
