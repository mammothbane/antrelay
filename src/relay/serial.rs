use std::{
    borrow::Borrow,
    sync::Arc,
};

use async_std::prelude::Stream;
use backoff::backoff::Backoff;
use eyre::WrapErr;
use futures::{
    AsyncRead,
    AsyncWrite,
};
use smol::prelude::*;
use tap::Pipe;

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
    stream_unwrap,
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

#[inline]
#[tracing::instrument(skip_all)]
pub fn wrap_relay_packets<'a, Env>(
    packets: impl Stream<Item = Vec<u8>>,
    status: impl Stream<Item = RealtimeStatus>,
) -> impl Stream<Item = Message<RelayPacket>>
where
    Env: PacketEnv<'a, 'a>,
{
    packets.zip(status).map(|(packet, status)| {
        Message::new(Header::downlink::<Env>(Conversation::Relay), RelayPacket {
            header:  status,
            payload: packet,
        })
    })
}
