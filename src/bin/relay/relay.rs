use std::time::Duration;

use eyre::WrapErr;
use smol::{
    io::AsyncWrite,
    stream::{
        Stream,
        StreamExt,
    },
};
use tap::Pipe;

use lunarrelay::{
    message::{
        crc_wrap::Ack,
        Message,
        OpaqueBytes,
    },
    packet_io::PacketIO,
    util,
};

lazy_static::lazy_static! {
    static ref SERIAL_BACKOFF: backoff::exponential::ExponentialBackoff<backoff::SystemClock> = backoff::ExponentialBackoffBuilder::new()
        .with_initial_interval(Duration::from_millis(25))
        .with_max_interval(Duration::from_secs(2))
        .with_randomization_factor(0.5)
        .with_max_elapsed_time(Some(Duration::from_secs(3)))
        .build();
}

pub async fn relay_uplink_to_serial<'a, 'u, R, W>(
    uplink: impl Stream<Item = Message<OpaqueBytes>> + 'u,
    packetio: &'a PacketIO<R, W>,
) -> impl Stream<Item = Message<Ack>> + 'a
where
    W: AsyncWrite + Unpin,
    'u: 'a,
{
    let backoff = SERIAL_BACKOFF.clone();

    uplink
        .filter(|msg| msg.header.destination != lunarrelay::message::header::Destination::Frontend)
        .map(|msg| -> eyre::Result<_> {
            let crc = msg.payload.checksum()?.to_vec();

            Ok((msg, crc))
        })
        .pipe(|s| util::log_and_discard_errors(s, "computing incoming message checksum"))
        .then(move |(msg, crc): (Message<OpaqueBytes>, Vec<u8>)| {
            backoff::future::retry_notify(
                backoff.clone(),
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
                    tracing::error!(error = %e, ?dur, "retrieving from serial");
                },
            )
        })
        .filter_map(|result| {
            lunarrelay::trace_catch!(result, "no ack from serial connection");

            result.ok()
        })
}
