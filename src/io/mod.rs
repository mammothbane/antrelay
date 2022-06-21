use eyre::WrapErr;
use smol::{
    io::AsyncWrite,
    prelude::*,
};
use tracing::Instrument;

mod command_sequencer;

use crate::futures::StreamExt as _;
pub use command_sequencer::CommandSequencer;

pub fn split_packets<'r, 'o>(
    r: impl AsyncBufRead + Unpin + 'r,
    sentinel: Vec<u8>,
    _max_packet_hint: usize,
) -> impl Stream<Item = eyre::Result<Vec<u8>>> + 'o
where
    'r: 'o,
{
    let buf = sentinel.clone();

    smol::stream::unfold((buf, r, sentinel), move |(mut buf, mut r, sentinel)| {
        Box::pin(async move {
            let result: eyre::Result<Vec<u8>> = try {
                loop {
                    let count = r
                        .read_until(*sentinel.last().expect("sentinel empty"), &mut buf)
                        .await
                        .wrap_err("splitting packet")?;

                    if count == 0 {
                        return None;
                    }

                    tracing::trace!(count, content = ?&buf[..count], "read message");

                    if buf.ends_with(&sentinel) {
                        break;
                    }
                }

                buf.drain(..(buf.len() - sentinel.len())).collect::<Vec<u8>>()
            };

            Some((result, (buf, r, sentinel)))
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

#[tracing::instrument(level = "trace", fields(packet = ?packet), err(Display), ret)]
#[cfg(feature = "serial_cobs")]
pub fn unpack_cobs(mut packet: Vec<u8>, sentinel: u8) -> eyre::Result<Vec<u8>> {
    let len_without_sentinel = packet.len().max(1) - 1;
    let new_len =
        cobs::decode_in_place_with_sentinel(&mut packet[..len_without_sentinel], sentinel)
            .map_err(|()| eyre::eyre!("cobs decode failed"))?;

    packet.truncate(new_len);

    Ok(packet)
}

#[tracing::instrument(level = "trace", fields(packet = ?packet), ret)]
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
