use actix::prelude::*;
use actix_broker::BrokerIssue;
use bytes::BytesMut;

use message::{BytesWrap, Header, header::Event, Message, MissionEpoch, RTParams};
use message::header::{Destination, Server};

use crate::{
    ground,
    serial,
};

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, derive_more::Display)]
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
    state: State,
}

impl Actor for StateMachine {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {}
}

impl Handler<ground::UpCommand> for StateMachine {
    type Result = ();

    fn handle(&mut self, msg: ground::UpCommand, ctx: &mut Self::Context) -> Self::Result {
        let msg = msg.0.as_ref();
        let event_type = msg.header.ty.event;

        let result = match (self.state, event_type) {
            (State::FlightIdle, Event::FE_5V_SUP) => {
                self.state = State::PingCentralStation,
            },

            (State::PingCentralStation, Event::FE_GARAGE_OPEN) => {
                self.state = State::StartBLE,
            },

            (State::StartBLE, Event::CS_GARAGE_OPEN) => {
                self.state = State::PollAnt,
            },

            (State::PollAnt, Event::FE_ROVER_STOP) => {},

            (State::CalibrateIMU, Event::A_CALI) => {

            },

            (State::CalibrateIMU, Event::FE_ROVER_MOVE) => {

            },

            #[cfg(debug_assertions)]
            (_, Event::FE_CS_PING) => {
                let msg = message::new(Header::command(RTParams {
                    time: MissionEpoch::now(),
                    seq: 0,
                }, Destination::CentralStation, Server::CentralStation, Event::CS_PING), BytesWrap::from(&[0]));

                self.issue_sync(serial::UpMessage(msg), ctx);
            },



            State::StartBLE => {
                send(
                    "ble start",
                    CRCMessage::new(Header::cs_command(env.borrow(), Event::CS_GARAGE_OPEN), vec![0]),
                    backoff,
                    csq,
                )
                    .await?;

                Some(State::PollAnt)
            },

            // TODO: how do we know we're done? do we continue polling until we get the rover stationary?
            State::PollAnt => {
                let env = env.borrow();
                let csq = csq.borrow();
                let uplink_messages = &mut uplink_messages;

                loop {
                    let result = util::either(
                        send(
                            "ant_ping",
                            CRCMessage::new(Header::ant_command(env, Event::A_PING), vec![0]),
                            backoff.clone(),
                            csq,
                        ),
                        uplink_messages
                            .filter(|msg| msg.header.ty.event == Event::FE_ROVER_STOP)
                            .next(),
                    )
                        .await;

                    match result {
                        either::Left(ping) => {
                            ping?;
                            continue;
                        },
                        either::Right(_rover_stopping) => break Some(State::CalibrateIMU),
                    }
                }
            },

            State::CalibrateIMU => {
                match util::either(
                    uplink_messages.filter(|msg| msg.header.ty.event == Event::FE_ROVER_MOVE).next(),
                    send(
                        "calibrate imu",
                        CRCMessage::new(Header::ant_command(env.borrow(), Event::A_CALI), vec![0]),
                        backoff,
                        csq,
                    ),
                )
                    .await
                {
                    either::Left(None) => return Ok(None),
                    either::Left(_) => Some(State::PollAnt),
                    either::Right(send_result) => {
                        send_result?;
                        Some(State::AntRun)
                    },
                }
            },

            State::AntRun => {
                if let None = uplink_messages
                    .filter(|msg| msg.header.ty.event == Event::FE_ROVER_MOVE)
                    .next()
                    .await
                {
                    return Ok(None);
                }

                Some(State::PollAnt)
            },
        };

    }
}

impl Handler<crate::serial::DownMessage> for StateMachine {
    type Result = ();

    fn handle(&mut self, msg: crate::serial::DownMessage, ctx: &mut Self::Context) -> Self::Result {
        todo!()
    }
}
