use std::{
    borrow::Borrow,
    sync::Arc,
    time::Duration,
};

use backoff::backoff::Backoff;
use eyre::WrapErr;
use futures::{
    AsyncRead,
    AsyncWrite,
};
use smol::prelude::*;
use tap::{
    Conv,
    Pipe,
};

use crate::{
    io::CommandSequencer,
    message::{
        header::{
            Conversation,
            Destination,
        },
        payload::{
            realtime_status::RealtimeStatus,
            Ack,
            RelayPacket,
        },
        Header,
        Message,
        OpaqueBytes,
    },
    relay::control::StateVal,
    stream_unwrap,
    util,
    util::PacketEnv,
};

#[tracing::instrument(level = "debug")]
pub async fn connect_serial(
    path: String,
    baud: u32,
) -> eyre::Result<(impl AsyncRead + Unpin, impl AsyncWrite + Unpin)> {
    let builder = tokio_serial::new(&path, baud);

    let stream =
        async_compat::Compat::new(async { tokio_serial::SerialStream::open(&builder) }).await?;

    let stream = async_compat::Compat::new(stream);
    Ok(smol::io::split(stream))
}

#[tracing::instrument(skip_all)]
pub fn relay_uplink_to_serial<'a, 'c>(
    uplink: impl Stream<Item = Message<OpaqueBytes>> + 'a,
    csq: impl Borrow<CommandSequencer> + 'c,
    request_backoff: impl Backoff + Clone + 'a,
) -> impl Stream<Item = Message<Ack>> + 'a
where
    'c: 'a,
{
    let csq = Arc::new(csq);

    uplink
        .filter(|msg| msg.header.destination != Destination::Frontend)
        .map(|msg| -> eyre::Result<_> {
            let crc = msg.payload.checksum()?.to_vec();

            Ok((msg, crc))
        })
        .pipe(stream_unwrap!("computing incoming message checksum"))
        .scan(csq, move |csq, (msg, crc)| {
            let csq = csq.clone();

            Some(Box::pin(backoff::future::retry_notify(
                request_backoff.clone(),
                move || {
                    let csq = csq.clone();
                    let msg = msg.clone();
                    let crc = crc.clone();

                    async move {
                        let csq = csq.as_ref().borrow();
                        let ret = csq.submit(msg).await.wrap_err("sending command to serial")?;

                        if ret.header.ty.request_was_invalid {
                            return Err(backoff::Error::transient(eyre::eyre!(
                                "ack message had invalid bit"
                            )));
                        }

                        if &[ret.payload.as_ref().checksum] != crc.as_slice() {
                            return Err(backoff::Error::transient(eyre::eyre!(
                                "ack message without invalid mismatch bit but mismatching checksum"
                            )));
                        }

                        Ok(ret) as Result<Message<Ack>, backoff::Error<eyre::Report>>
                    }
                },
                |e, dur| {
                    tracing::error!(error = %e, backoff_dur = ?dur, "retrieving from serial");
                },
            )))
        })
        .then(|s| s)
        .pipe(stream_unwrap!("no ack from serial connection"))
}

async fn send(
    desc: &str,
    msg: Message<OpaqueBytes>,
    backoff: impl Backoff + Clone,
    csq: impl Borrow<CommandSequencer>,
) -> eyre::Result<Message<Ack>> {
    let csq = csq.borrow();

    let result = backoff::future::retry_notify(backoff, move || {
        let msg = msg.clone();

        async move {
            let result = csq.submit(msg).await.map_err(backoff::Error::transient)?;

            if result.header.ty.request_was_invalid {
                return Err(backoff::Error::transient(eyre::eyre!("cs reports '{}' invalid", desc)));
            }

            Ok(result)
        }
    }, |e, dur| {
        tracing::warn!(error = %e, backoff_dur = ?dur, "submitting '{}' to central station", desc);
    }).await?;

    Ok(result)
}

async fn handle_step(
    env: impl Borrow<PacketEnv>,
    state: StateVal,
    mut uplink_messages: impl Stream<Item = Message<OpaqueBytes>> + Unpin,
    backoff: impl Backoff + Clone,
    csq: impl Borrow<CommandSequencer>,
) -> eyre::Result<StateVal> {
    let result = match state {
        StateVal::FlightIdle => loop {
            let msg = uplink_messages
                .filter(|msg| msg.header.ty.conversation_type == Conversation::VoltageSupplied)
                .next()
                .await
                .ok_or(eyre::eyre!("message stream ended"))?;

            send("5v_sup", msg, backoff, csq).await?;
            break StateVal::PingCentralStation;
        },

        StateVal::PingCentralStation => {
            let uplink_messages = &mut uplink_messages;

            loop {
                let env = env.borrow();
                let send = &send;

                let backoff = backoff.clone();
                let csq = csq.borrow();

                let ping = async move {
                    send(
                        "ping",
                        Message::new(Header::cs_command(env, Conversation::Ping), vec![]),
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
                    either::Left(garage_open_msg) => {
                        garage_open_msg.ok_or(eyre::eyre!("uplink stream closed"))?
                    },
                };

                break StateVal::StartBLE;
            }
        },

        StateVal::StartBLE => {
            drop(uplink_messages);

            send(
                "ble start",
                Message::new(Header::cs_command(env.borrow(), Conversation::Start), vec![]),
                backoff,
                csq,
            )
            .await?;

            StateVal::AwaitRoverStationary
        },

        StateVal::AwaitRoverStationary => {
            // TODO: notify stop moving?

            uplink_messages
                .filter(|msg| msg.header.ty.conversation_type == Conversation::RoverHalting)
                .next()
                .await
                .ok_or(eyre::eyre!("uplink stream closed"))?;

            StateVal::CalibrateIMU
        },

        StateVal::CalibrateIMU => {
            match util::either(
                uplink_messages
                    .filter(|msg| msg.header.ty.conversation_type == Conversation::RoverWillTurn)
                    .next(),
                send(
                    "calibrate imu",
                    Message::new(Header::cs_command(env.borrow(), Conversation::Calibrate), vec![]),
                    backoff,
                    csq,
                ),
            )
            .await
            {
                either::Left(will_turn) => {
                    will_turn.ok_or(eyre::eyre!("uplink stream closed"))?;
                    StateVal::AwaitRoverStationary
                },
                either::Right(send_result) => {
                    send_result?;
                    StateVal::AntRun
                },
            }
        },

        StateVal::AntRun => {
            uplink_messages
                .filter(|msg| msg.header.ty.conversation_type == Conversation::RoverWillTurn)
                .next()
                .await
                .ok_or(eyre::eyre!("uplink stream closed"))?;

            StateVal::AwaitRoverStationary
        },
    };

    Ok(result)
}

#[inline]
#[tracing::instrument(skip_all)]
pub fn wrap_relay_packets<'e, 'p, 's, 'o>(
    env: impl Borrow<PacketEnv> + 'e,
    packets: impl Stream<Item = Vec<u8>> + 'p,
    status: impl Stream<Item = RealtimeStatus> + 's,
) -> impl Stream<Item = Message<RelayPacket>> + 'o
where
    'e: 'o,
    'p: 'o,
    's: 'o,
{
    packets.zip(status).map(move |(packet, status)| {
        Message::new(Header::downlink(env.borrow(), Conversation::Relay), RelayPacket {
            header:  status,
            payload: packet,
        })
    })
}
