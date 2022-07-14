use actix::{
    Actor,
    ActorContext,
    ArbiterHandle,
    AsyncContext,
    Context,
    Running,
    Supervised,
};
use bytes::{
    Buf,
    BytesMut,
};
use eyre::WrapErr;
use packed_struct::PackedStructSlice;
use std::marker::PhantomData;
use tokio::io::{
    AsyncBufRead,
    AsyncWrite,
};
use tokio_util::codec::{
    Decoder,
    Encoder,
};
use tracing::Instrument;

use crate::futures::StreamExt as _;

pub mod codec;

pub struct SerialIdent(u64);

pub struct SerialReader {
    builder: tokio_serial::SerialPortBuilder,
}

impl Actor for SerialReader {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let Ok(serial_stream) = tokio_serial::SerialStream::open(&self.builder) else {
            ctx.stop();
            return;
        };

        let null_delimited = tokio_util::codec::AnyDelimiterCodec::new(vec![0u8], vec![0u8]);

        ctx.spawn(async move {});

        self.builder.clone();
        tokio_serial::new(self.path, self.baud);

        actix::spawn(async move {
            tokio_serial::new(&self.0);
            self.0
        });
    }
}

impl Supervised for SerialReader {}

pub fn split_packets<'r, 'o>(
    r: impl AsyncBufRead + Unpin + 'r,
    sentinel: Vec<u8>,
    _max_packet_hint: usize,
) -> impl Stream<Item = eyre::Result<Vec<u8>>> + 'o
where
    'r: 'o,
{
    #[cfg(not(feature = "serial_cobs"))]
    let buf = sentinel.clone();

    #[cfg(feature = "serial_cobs")]
    let buf = vec![];

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

                let result = buf.drain(..(buf.len() - sentinel.len())).collect::<Vec<u8>>();

                #[cfg(feature = "serial_cobs")]
                {
                    buf.truncate(0);
                }

                tracing::debug!(content_hex = %hex::encode(&result), "split serial packet");

                result
            };

            Some((result, (buf, r, sentinel)))
        })
    })
    .fuse()
}
