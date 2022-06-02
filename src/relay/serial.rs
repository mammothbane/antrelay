use std::sync::Arc;

use async_std::prelude::Stream;
use backoff::backoff::Backoff;
use eyre::WrapErr;
use futures::{
    AsyncRead,
    AsyncWrite,
};
use smol::stream::StreamExt;
use tap::Pipe;

use crate::{
    message::{
        crc_wrap::{
            Ack,
            RealtimeStatus,
        },
        header::{
            Destination,
            Kind,
            Source,
            Type,
        },
        payload::RelayPacket,
        CRCWrap,
        Header,
        Message,
        OpaqueBytes,
    },
    packet_io::PacketIO,
    stream_unwrap,
    MissionEpoch,
};

#[tracing::instrument]
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
pub fn relay_uplink_to_serial<'a, 'u, R, W>(
    uplink: impl Stream<Item = Message<OpaqueBytes>> + 'u,
    packetio: Arc<PacketIO<R, W>>,
    request_backoff: impl Backoff + Clone + 'static,
) -> impl Stream<Item = Message<Ack>> + 'a
where
    W: AsyncWrite + Unpin + 'u,
    R: 'a,
    'u: 'a,
{
    uplink
        .filter(|msg| msg.header.destination != Destination::Frontend)
        .map(|msg| -> eyre::Result<_> {
            let crc = msg.payload.checksum()?.to_vec();

            Ok((msg, crc))
        })
        .pipe(stream_unwrap!("computing incoming message checksum"))
        .then(move |(msg, crc): (Message<OpaqueBytes>, Vec<u8>)| {
            let packetio = packetio.clone();

            Box::pin(backoff::future::retry_notify(
                request_backoff.clone(),
                move || {
                    let msg = msg.clone();
                    let crc = crc.clone();
                    let packetio = packetio.clone();

                    async move {
                        let g = packetio.request(&msg).await.wrap_err("sending serial request")?;
                        let ret = g.wait().await.wrap_err("receiving serial response")?;

                        if ret.header.ty.acked_message_invalid {
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
            ))
        })
        .pipe(stream_unwrap!("no ack from serial connection"))
}

#[inline]
#[tracing::instrument(skip_all)]
pub fn wrap_relay_packets(
    packets: impl Stream<Item = Vec<u8>>,
    status: impl Stream<Item = RealtimeStatus>,
) -> impl Stream<Item = Message<RelayPacket>> {
    packets.zip(status).map(|(packet, status)| Message {
        header:  Header {
            magic:       Default::default(),
            destination: Destination::Ground,
            timestamp:   MissionEpoch::now(),
            seq:         0, // TODO
            ty:          Type {
                ack:                   false,
                acked_message_invalid: false,
                source:                Source::Frontend,
                kind:                  Kind::Relay,
            },
        },
        payload: CRCWrap::new(RelayPacket {
            header:  status,
            payload: packet,
        }),
    })
}
