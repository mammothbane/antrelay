use eyre::WrapErr;
use smol::{
    io::AsyncWrite,
    prelude::*,
};
use std::{
    cell::RefCell,
    rc::Rc,
};

mod command_sequencer;

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

pub fn write_packet_stream(
    s: impl Stream<Item = Vec<u8>>,
    w: impl AsyncWrite + Unpin + 'static,
) -> impl Stream<Item = eyre::Result<()>> {
    let w = Rc::new(RefCell::new(w));

    s.then(move |pkt| {
        let w = w.clone();

        async move {
            let mut w = w.borrow_mut();

            let pkt = pkt;
            w.write_all(&pkt).await
        }
    })
    .map(|result| result.map_err(eyre::Report::from))
}
