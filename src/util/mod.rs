use std::{
    future::Future,
    time::Duration,
};

use packed_struct::PackedStructSlice;
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

#[inline(always)]
pub fn compose<A, B, C, G, F>(f: F, g: G) -> impl Fn(A) -> C
where
    F: Fn(A) -> B,
    G: Fn(B) -> C,
{
    move |x| g(f(x))
}

#[macro_export]
macro_rules! compose {
    ($c:ident, $f:expr, $g:expr $(, $gs:expr)+) => {
        move |x| $f(x).$c($crate::compose!($c, $g, $($gs,)+))
    };

    ($c:ident, $f:expr, $g:expr) => {
        move |x| $f(x).$c(&$g)
    };

    ($c:ident, $($args:expr,)*) => {
        $crate::compose!($c $(, $args)*)
    };
}

#[macro_export]
macro_rules! fold {
    ($c:expr, $f:expr, $gnext:expr $(, $gs:expr)+) => {
        $c($f, $crate::fold!($c, $gnext $(, $gs)+))
    };

    ($c:expr, $f:expr, $g:expr,) => {
        $crate::fold!($c, $f, $g)
    };

    ($c:expr, $f:expr, $g:expr) => {
        $c($f, $g)
    };
}

#[inline]
#[tracing::instrument(skip(f), level = "trace")]
pub async fn timeout<T>(millis: u64, f: impl Future<Output = T>) -> eyre::Result<T> {
    match either(f, smol::Timer::after(Duration::from_millis(millis))).await {
        either::Left(t) => Ok(t),
        either::Right(_timeout) => Err(eyre::eyre!("timed out")),
    }
}

#[inline]
#[tracing::instrument(skip_all, level = "trace")]
pub async fn either<T, U>(
    t: impl Future<Output = T>,
    u: impl Future<Output = U>,
) -> either::Either<T, U> {
    smol::future::or(async move { either::Left(t.await) }, async move { either::Right(u.await) })
        .await
}

#[tracing::instrument(skip(s), level = "trace")]
pub fn splittable_stream<'s, 'o, S>(
    s: S,
    buffer: usize,
) -> (impl Stream<Item = S::Item> + Clone + Unpin + 'o, impl Future<Output = ()> + 's)
where
    S: Stream + Send + 's,
    S::Item: Clone + Send + Sync + 'o,
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
pub fn unpack_message<T>(v: Vec<u8>) -> eyre::Result<Message<T>>
where
    T: PackedStructSlice,
{
    <Message<T> as PackedStructSlice>::unpack_from_slice(&v).map_err(eyre::Report::from)
}

#[inline]
pub fn pack_message<T>(t: T) -> eyre::Result<Vec<u8>>
where
    T: PackedStructSlice,
{
    t.pack_to_vec().map_err(eyre::Report::from)
}

#[tracing::instrument(skip_all, level = "trace")]
pub fn brotli_compress(message: Vec<u8>) -> eyre::Result<Vec<u8>> {
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

pub fn brotli_decompress(v: Vec<u8>) -> eyre::Result<Vec<u8>> {
    let mut out = vec![];
    brotli::BrotliDecompress(&mut &v[..], &mut out).map_err(eyre::Report::from)?;

    Ok(out)
}
