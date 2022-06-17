use chrono::Utc;
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
    io::{
        pack_cobs,
        unpack_cobs,
    },
    message::Message,
    trace_catch,
};

mod clock;
#[cfg(unix)]
pub mod dynload;
mod reader;
mod seq;
mod state;

use crate::message::OpaqueBytes;
pub use clock::Clock;
pub use reader::Reader;
pub use seq::{
    Const,
    Seq,
};

crate::atomic_seq!(pub U8Sequence);

type SerializeFunction<E> = dyn Fn(Message<OpaqueBytes>) -> Result<Vec<u8>, E> + Send + Sync;
type DeserializeFunction<E> = dyn Fn(Vec<u8>) -> Result<Message<OpaqueBytes>, E> + Send + Sync;

pub struct SerialCodec<E = eyre::Report> {
    pub serialize:   Box<SerializeFunction<E>>,
    pub deserialize: Box<DeserializeFunction<E>>,
    pub sentinel:    u8,
}

pub struct GroundLinkCodec<E = eyre::Report> {
    pub serialize:   Box<SerializeFunction<E>>,
    pub deserialize: Box<DeserializeFunction<E>>,
}

pub struct PacketEnv {
    pub clock: Box<dyn Clock + Send + Sync>,
    pub seq:   Box<dyn Seq<Output = u8> + Send + Sync>,
}

impl Default for PacketEnv {
    fn default() -> Self {
        Self {
            clock: Box::new(Utc),
            seq:   Box::new(U8Sequence::new()),
        }
    }
}

pub const DEFAULT_COBS_SENTINEL: u8 = 0u8;

lazy_static::lazy_static! {
    pub static ref DEFAULT_GROUND_CODEC: GroundLinkCodec = GroundLinkCodec {
        serialize:   Box::new(crate::compose!(
            and_then,
            pack_message,
            brotli_compress
        )),
        deserialize: Box::new(crate::compose!(and_then, brotli_decompress, unpack_message)),
    };

    pub static ref DEFAULT_SERIAL_CODEC: SerialCodec = SerialCodec {
        serialize:   Box::new(crate::compose!(
            map,
            pack_message,
            |v| pack_cobs(v, DEFAULT_COBS_SENTINEL)
        )),
        deserialize: Box::new(crate::compose!(
            and_then,
            |v| unpack_cobs(v, DEFAULT_COBS_SENTINEL),
            unpack_message
        )),
        sentinel:    DEFAULT_COBS_SENTINEL,
    };
}

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
