use std::{
    borrow::Borrow,
    time::Duration,
};

use backoff::backoff::Backoff;
use smol::prelude::*;

use crate::{
    io::CommandSequencer,
    message::{
        header::Event,
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
                .filter(|msg| msg.header.ty.event == Event::FE_5V_SUP)
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
                        CRCMessage::new(Header::cs_command(env, Event::CS_PING), vec![0]),
                        backoff,
                        csq,
                    )
                    .await?;

                    smol::Timer::after(Duration::from_secs(5)).await;

                    Ok(()) as eyre::Result<()>
                };

                let _garage_open = match util::either(
                    uplink_messages
                        .filter(|msg| msg.header.ty.event == Event::FE_GARAGE_OPEN)
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

    Ok(result)
}
