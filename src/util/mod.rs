use std::future::Future;

use packed_struct::{
    PackedStructSlice,
    PackingResult,
};
use smol::stream::{
    Stream,
    StreamExt,
};
use tracing::Span;

use crate::message::Message;

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

// TODO: task to future
#[tracing::instrument(skip(s))]
pub fn splittable_stream<S>(s: S, buffer: usize) -> impl Stream<Item = S::Item> + Clone + Unpin
where
    S: Stream + Send + 'static,
    S::Item: Clone + Send + Sync,
{
    let (tx, rx) = {
        let (mut tx, rx) = async_broadcast::broadcast(buffer);
        tx.set_overflow(true);

        (tx, rx)
    };

    let span = Span::current();

    smol::spawn(async move {
        let tx = tx;
        let span = span;

        s.for_each(|s| {
            trace_catch!(parent: &span, tx.try_broadcast(s), "broadcasting to splittable stream");
        })
        .await;
    })
    .detach();

    rx
}

#[inline]
#[tracing::instrument(skip_all)]
pub fn log_and_discard_errors<T, E>(
    s: impl Stream<Item = Result<T, E>>,
    msg: &'static str,
) -> impl Stream<Item = T>
where
    E: std::fmt::Display,
{
    s.filter_map(move |x| {
        trace_catch!(x, msg);

        x.ok()
    })
}

#[macro_export]
macro_rules! stream_unwrap {
    ($($args:tt)*) => {
        |s| $crate::util::log_and_discard_errors(s, $($args)*)
    };
}

#[macro_export]
macro_rules! bootstrap {
    ($x:expr $( , $xs:expr )* $(,)?) => {
        eprintln!(concat!("[bootstrap] ", $x) $( , $xs )*)
    };
}

#[macro_export]
macro_rules! trace_catch {
    (parent: $parent:expr, $val:expr, $($rest:tt)*) => {
        if let Err(ref e) = $val {
            ::tracing::error!(parent: $parent, error = %e, $($rest)*);
        }
    };

    ($val:expr, $($rest:tt)*) => {
        if let Err(ref e) = $val {
            ::tracing::error!(error = %e, $($rest)*);
        }
    };
}

pub use bootstrap;
pub use trace_catch;

#[inline]
#[tracing::instrument(skip_all, level = "debug")]
pub fn deserialize_messages<T>(
    s: impl Stream<Item = Vec<u8>>,
) -> impl Stream<Item = PackingResult<Message<T>>>
where
    T: PackedStructSlice,
{
    s.map(|msg| <Message<T> as PackedStructSlice>::unpack_from_slice(&msg))
}

#[tracing::instrument(skip_all, level = "debug")]
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
