mod command_sequencer;

use crate::message::{
    payload::Ack,
    Message,
};
use eyre::WrapErr;
use futures::AsyncWriteExt;
use packed_struct::PackedStructSlice;
use smol::{
    io::{
        AsyncBufRead,
        AsyncBufReadExt as _,
        AsyncWrite,
    },
    stream::{
        Stream,
        StreamExt,
    },
};

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
                tracing::trace!("waiting for serial message");
                let count = r.read_until(sentinel, &mut buf).await.wrap_err("splitting packet")?;

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

#[cfg(feature = "serial_cobs")]
pub fn unpack_cobs(mut packet: Vec<u8>, sentinel: u8) -> eyre::Result<Vec<u8>> {
    let new_len = cobs::decode_in_place_with_sentinel(&mut packet, sentinel)
        .map_err(|()| eyre::eyre!("cobs decode failed"))?;

    packet.truncate(new_len);

    Ok(packet)
}

#[cfg(feature = "serial_cobs")]
pub fn pack_cobs_stream(
    s: impl Stream<Item = Vec<u8>>,
    sentinel: u8,
) -> impl Stream<Item = Vec<u8>> {
    s.map(move |packet| cobs::encode_vec_with_sentinel(&packet, sentinel))
}
