use std::{
    sync::atomic::AtomicU8,
    time::Duration,
};

use futures::AsyncRead;
use packed_struct::PackedStructSlice;
use smol::{
    io::AsyncBufReadExt,
    stream::{
        Stream,
        StreamExt,
    },
};

use lunarrelay::message::{
    self,
    OpaqueBytes,
};
pub use sockets::DownlinkSockets;

mod sockets;

const SENTINEL: u8 = 0;

lazy_static::lazy_static! {
    static ref SEQUENCE_NUMBER: AtomicU8 = AtomicU8::new(0);
}

#[derive(Clone, Debug)]
enum DownlinkStatus {
    Continue(message::Message<OpaqueBytes>),
    Close,
}

#[tracing::instrument(skip_all)]
pub async fn downlink(
    downlink_sockets: DownlinkSockets,
    serial_read: impl AsyncRead + Unpin,
    done: smol::channel::Receiver<!>,
) -> impl Stream<Item = message::Message<OpaqueBytes>> {
    let mut serial_read = smol::io::BufReader::new(serial_read);

    loop {
        match downlink_once(&downlink_sockets, &mut serial_read, &mut buf, &done).await {
            Ok(DownlinkStatus::Continue(_msg)) => {
                todo!();
            },
            Ok(DownlinkStatus::Close) => break,
            Err(e) => {
                tracing::error!(error = ?e, "downlink error, sleeping before retry");
                smol::Timer::after(Duration::from_millis(20)).await;
            },
        }
    }

    todo!()
}

pub fn stream_serial_messages() -> impl Stream<Item = message::Message<OpaqueBytes>> {
    let mut buf = vec![0; 4096];

    smol::stream::repeat_with(|| receive_serial_message(todo!(), &mut buf))
        .then(|msg| async move {
            let contents = msg.await;
        })
        .map(|elt| todo!())
}

pub async fn receive_serial_message(
    serial_read: &mut (impl AsyncBufReadExt + Unpin),
    buf: &mut Vec<u8>,
) -> eyre::Result<Vec<u8>> {
    let count = serial_read.read_until(SENTINEL, buf).await?;

    // TODO: framing pending fz choice re: cobs
    let data = {
        cfg_if::cfg_if! {
            if #[cfg(feature = "serial_cobs")] {
                cobs::decode_vec(&mut buf[..count])?
            } else {
                Vec::from(&buf[..count])
            }
        }
    };

    Ok(data)
}

pub fn pack_downlink_message(message: &message::Message<OpaqueBytes>) -> eyre::Result<Vec<u8>> {
    let packed_message = message.pack_to_vec()?;

    let compressed_bytes = {
        let mut out = vec![];

        lazy_static::lazy_static! {
            static ref PARAMS: brotli::enc::BrotliEncoderParams = brotli::enc::BrotliEncoderParams {
                quality: 11,
                ..Default::default()
            };
        }

        brotli::BrotliCompress(&mut &packed_message[..], &mut out, &*PARAMS)?;

        out
    };

    Ok(compressed_bytes)
}
