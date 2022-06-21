use std::{
    borrow::Borrow,
    time::Duration,
};

use backoff::backoff::Backoff;
use smol::prelude::*;

use crate::{
    io::CommandSequencer,
    message::{
        header::Conversation,
        CRCMessage,
        Header,
        OpaqueBytes,
    },
    util::{
        self,
        send,
        PacketEnv,
    },
};

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, derive_more::Display)]
#[repr(u8)]
pub enum State {
    FlightIdle,
    PingCentralStation,
    StartBLE,
    PollAnt,
    AwaitRoverStationary,
    CalibrateIMU,
    AntRun,
}

#[tracing::instrument(skip(env, uplink_messages, backoff, csq), ret, err(Display), level = "debug")]
pub async fn state_machine(
    env: impl Borrow<PacketEnv>,
    state: State,
    mut uplink_messages: impl Stream<Item = CRCMessage<OpaqueBytes>> + Unpin,
    backoff: impl Backoff + Clone,
    csq: impl Borrow<CommandSequencer>,
) -> eyre::Result<Option<State>> {
    let result = match state {
        State::FlightIdle => {
            let Some(_) = uplink_messages
                .filter(|msg| msg.header.ty.conversation_type == Conversation::PowerSupplied)
                .next()
                .await else {
                return Ok(None);
            };

            Some(State::PingCentralStation)
        },

        State::PingCentralStation => {
            let uplink_messages = &mut uplink_messages;
            let env = env.borrow();
            let csq = csq.borrow();

            loop {
                let backoff = backoff.clone();

                let ping = async move {
                    send(
                        "ping",
                        CRCMessage::new(Header::cs_command(env, Conversation::Ping), vec![]),
                        backoff,
                        csq,
                    )
                    .await?;
                    smol::Timer::after(Duration::from_secs(30)).await;

                    Ok(()) as eyre::Result<()>
                };

                let _garage_open = match util::either(
                    uplink_messages
                        .filter(|msg| {
                            msg.header.ty.conversation_type == Conversation::GarageOpenPending
                        })
                        .next(),
                    ping,
                )
                .await
                {
                    either::Right(_) => continue,
                    either::Left(None) => {
                        return Ok(None);
                    },
                    either::Left(_) => {},
                };

                break Some(State::StartBLE);
            }
        },

        State::StartBLE => {
            send(
                "ble start",
                CRCMessage::new(Header::cs_command(env.borrow(), Conversation::Start), vec![]),
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

            loop {
                let _result = send(
                    "ant_ping",
                    CRCMessage::new(Header::ant_command(env, Conversation::Ping), vec![]),
                    backoff.clone(),
                    csq,
                )
                .await?;

                break;
            }

            Some(State::AwaitRoverStationary)
        },

        State::AwaitRoverStationary => {
            // TODO: notify stop moving?

            if let None = uplink_messages
                .filter(|msg| msg.header.ty.conversation_type == Conversation::RoverStopping)
                .next()
                .await
            {
                return Ok(None);
            }

            Some(State::CalibrateIMU)
        },

        State::CalibrateIMU => {
            match util::either(
                uplink_messages
                    .filter(|msg| msg.header.ty.conversation_type == Conversation::RoverMoving)
                    .next(),
                send(
                    "calibrate imu",
                    CRCMessage::new(
                        Header::cs_command(env.borrow(), Conversation::Calibrate),
                        vec![],
                    ),
                    backoff,
                    csq,
                ),
            )
            .await
            {
                either::Left(None) => return Ok(None),
                either::Left(_) => Some(State::AwaitRoverStationary),
                either::Right(send_result) => {
                    send_result?;
                    Some(State::AntRun)
                },
            }
        },

        State::AntRun => {
            if let None = uplink_messages
                .filter(|msg| msg.header.ty.conversation_type == Conversation::RoverMoving)
                .next()
                .await
            {
                return Ok(None);
            }

            Some(State::AwaitRoverStationary)
        },
    };

    Ok(result)
}
