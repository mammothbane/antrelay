use crate::{
    message::{
        crc_wrap::Ack,
        Message,
        OpaqueBytes,
    },
    packet_io::PacketIO,
    util::log_and_discard_errors,
};
use async_std::prelude::Stream;
use backoff::backoff::Backoff;
use eyre::WrapErr;
use futures::{
    AsyncRead,
    AsyncWrite,
};
use smol::stream::StreamExt;
use tap::Pipe;

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

pub async fn relay_uplink_to_serial<'a, 'u, R, W>(
    uplink: impl Stream<Item = Message<OpaqueBytes>> + 'u,
    packetio: &'a PacketIO<R, W>,
    request_backoff: impl Backoff + Clone + 'static,
) -> impl Stream<Item = Message<Ack>> + 'a
where
    W: AsyncWrite + Unpin,
    'u: 'a,
{
    uplink
        .filter(|msg| msg.header.destination != crate::message::header::Destination::Frontend)
        .map(|msg| -> eyre::Result<_> {
            let crc = msg.payload.checksum()?.to_vec();

            Ok((msg, crc))
        })
        .pipe(|s| log_and_discard_errors(s, "computing incoming message checksum"))
        .then(move |(msg, crc): (Message<OpaqueBytes>, Vec<u8>)| {
            Box::pin(backoff::future::retry_notify(
                request_backoff.clone(),
                move || {
                    let msg = msg.clone();
                    let crc = crc.clone();

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
        .pipe(|s| log_and_discard_errors(s, "no ack from serial connection"))
}
