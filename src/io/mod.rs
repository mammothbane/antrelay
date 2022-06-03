use eyre::WrapErr;
use smol::{
    io::AsyncWrite,
    prelude::*,
};
use tracing::Instrument;

mod command_sequencer;

use crate::futures::StreamExt as _;
pub use command_sequencer::CommandSequencer;

#[tracing::instrument(skip(r), level = "debug")]
pub fn split_packets(
    r: impl AsyncBufRead + Unpin + 'static,
    sentinel: u8,
    max_packet_hint: usize,
) -> impl Stream<Item = eyre::Result<Vec<u8>>> {
    smol::stream::unfold((Vec::with_capacity(max_packet_hint), r), move |(mut buf, mut r)| {
        buf.truncate(0);

        Box::pin(async move {
            let result: eyre::Result<Vec<u8>> = try {
                tracing::debug!("waiting for serial message");
                let count = r.read_until(sentinel, &mut buf).await.wrap_err("splitting packet")?;

                if count == 0 {
                    tracing::debug!("eof");
                    return None;
                }

                tracing::debug!(count, content = ?&buf[..count], "read message");

                buf[..count].to_vec()
            };

            Some((result, (buf, r)))
        })
    })
    .fuse()
}

#[cfg(feature = "serial_cobs")]
pub fn unpack_cobs_stream(
    s: impl Stream<Item = Vec<u8>>,
    sentinel: u8,
) -> impl Stream<Item = eyre::Result<Vec<u8>>> {
    s.map(move |data| unpack_cobs(data, sentinel))
}

#[tracing::instrument(level = "debug", fields(packet = ?packet), err(Display))]
#[cfg(feature = "serial_cobs")]
pub fn unpack_cobs(mut packet: Vec<u8>, sentinel: u8) -> eyre::Result<Vec<u8>> {
    let len_without_sentinel = packet.len().max(1) - 1;
    let new_len =
        cobs::decode_in_place_with_sentinel(&mut packet[..len_without_sentinel], sentinel)
            .map_err(|()| eyre::eyre!("cobs decode failed"))?;

    packet.truncate(new_len);

    Ok(packet)
}

#[cfg(feature = "serial_cobs")]
pub fn pack_cobs(packet: Vec<u8>, sentinel: u8) -> Vec<u8> {
    let mut ret = cobs::encode_vec_with_sentinel(&packet, sentinel);
    ret.push(sentinel);
    ret
}

#[cfg(feature = "serial_cobs")]
pub fn pack_cobs_stream(
    s: impl Stream<Item = Vec<u8>>,
    sentinel: u8,
) -> impl Stream<Item = Vec<u8>> {
    s.map(move |v| pack_cobs(v, sentinel))
}

#[tracing::instrument(skip_all, level = "debug")]
pub fn write_packet_stream(
    s: impl Stream<Item = Vec<u8>>,
    w: impl AsyncWrite + Unpin + 'static,
) -> impl Stream<Item = eyre::Result<()>> {
    s.owned_scan(w, |mut w, pkt| async move {
        let result = w
            .write_all(&pkt)
            .instrument(tracing::debug_span!("write packet", len = pkt.len(), content = ?pkt))
            .await;

        Some((w, result))
    })
    .map(|result| result.map_err(eyre::Report::from))
}
