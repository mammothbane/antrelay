use std::future::Future;

use packed_struct::{
    PackedStructSlice,
    PackingResult,
};
use smol::stream::{
    Stream,
    StreamExt,
};

use crate::{
    message::Message,
    trace_catch,
};

#[cfg(unix)]
pub mod dynload;

#[inline]
#[tracing::instrument(skip_all)]
pub async fn either<T, U>(
    t: impl Future<Output = T>,
    u: impl Future<Output = U>,
) -> either::Either<T, U> {
    smol::future::or(async move { either::Left(t.await) }, async move { either::Right(u.await) })
        .await
}

#[tracing::instrument(skip(s))]
pub fn splittable_stream<S>(
    s: S,
    buffer: usize,
) -> (impl Stream<Item = S::Item> + Clone + Unpin, impl Future<Output = ()>)
where
    S: Stream + Send + 'static,
    S::Item: Clone + Send + Sync,
{
    let (tx, rx) = {
        let (mut tx, rx) = async_broadcast::broadcast(buffer);
        tx.set_overflow(true);

        (tx, rx)
    };

    let pump = async move {
        let tx = tx;

        s.for_each(move |s| {
            trace_catch!(tx.try_broadcast(s), "broadcasting to splittable stream");
        })
        .await;
    };

    (rx, pump)
}

#[inline]
#[tracing::instrument(skip_all, level = "trace")]
pub fn deserialize_messages<T>(
    s: impl Stream<Item = Vec<u8>>,
) -> impl Stream<Item = PackingResult<Message<T>>>
where
    T: PackedStructSlice,
{
    s.map(|msg| <Message<T> as PackedStructSlice>::unpack_from_slice(&msg))
}

#[tracing::instrument(skip_all, level = "trace")]
pub fn brotli_compress(message: &mut Vec<u8>) -> eyre::Result<Vec<u8>> {
    let compressed_bytes = {
        let mut out = vec![];

        lazy_static::lazy_static! {
            static ref PARAMS: brotli::enc::BrotliEncoderParams = brotli::enc::BrotliEncoderParams {
                quality: 11,
                ..Default::default()
            };
        }

        brotli::BrotliCompress(&mut &message[..], &mut out, &*PARAMS)?;

        out
    };

    Ok(compressed_bytes)
}
