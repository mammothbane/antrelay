use backoff::backoff::Backoff;
use chrono::Utc;
use std::{
    borrow::Borrow,
    future::Future,
    sync::Arc,
    time::Duration,
};

use packed_struct::PackedStructSlice;
use smol::stream::{
    Stream,
    StreamExt,
};

use crate::{
    io::CommandSequencer,
    message::{
        payload::Ack,
        CRCMessage,
        OpaqueBytes,
    },
    trace_catch,
};

#[cfg(feature = "serial_cobs")]
use crate::io::{
    pack_cobs,
    unpack_cobs,
};

mod clock;
mod reader;
mod seq;
mod state;

pub use clock::Clock;
pub use reader::Reader;
pub use seq::{
    Const,
    Seq,
};

crate::atomic_seq!(pub U8Sequence);

type SerializeFunction<E> = dyn Fn(CRCMessage<OpaqueBytes>) -> Result<Vec<u8>, E> + Send + Sync;
type DeserializeFunction<E> = dyn Fn(Vec<u8>) -> Result<CRCMessage<OpaqueBytes>, E> + Send + Sync;

pub struct SerialCodec<E = eyre::Report> {
    pub encode:   Arc<SerializeFunction<E>>,
    pub decode:   Arc<DeserializeFunction<E>>,
    pub sentinel: Vec<u8>,
}

impl<E> Clone for SerialCodec<E> {
    fn clone(&self) -> Self {
        Self {
            encode:   self.encode.clone(),
            decode:   self.decode.clone(),
            sentinel: self.sentinel.clone(),
        }
    }
}

pub struct DownlinkCodec<E = eyre::Report> {
    pub encode: Arc<SerializeFunction<E>>,
    pub decode: Arc<DeserializeFunction<E>>,
}

impl<E> Clone for DownlinkCodec<E> {
    fn clone(&self) -> Self {
        Self {
            encode: self.encode.clone(),
            decode: self.decode.clone(),
        }
    }
}

pub struct UplinkCodec<E = eyre::Report> {
    pub encode: Arc<SerializeFunction<E>>,
    pub decode: Arc<DeserializeFunction<E>>,
}

impl<E> Clone for UplinkCodec<E> {
    fn clone(&self) -> Self {
        Self {
            encode: self.encode.clone(),
            decode: self.decode.clone(),
        }
    }
}

pub struct LinkCodecs<E = eyre::Report> {
    pub downlink: Arc<DownlinkCodec<E>>,
    pub uplink:   Arc<UplinkCodec<E>>,
}

impl<E> Clone for LinkCodecs<E> {
    fn clone(&self) -> Self {
        Self {
            downlink: self.downlink.clone(),
            uplink:   self.uplink.clone(),
        }
    }
}

#[derive(Clone)]
pub struct PacketEnv {
    pub clock: Arc<dyn Clock + Send + Sync>,
    pub seq:   Arc<dyn Seq<Output = u8> + Send + Sync>,
}

impl Default for PacketEnv {
    fn default() -> Self {
        Self {
            clock: Arc::new(Utc),
            seq:   Arc::new(U8Sequence::new()),
        }
    }
}

pub const DEFAULT_COBS_SENTINEL: u8 = 0u8;

lazy_static::lazy_static! {
    pub static ref DEFAULT_DOWNLINK_CODEC: DownlinkCodec = DownlinkCodec {
        encode:   Arc::new(crate::compose!(
            and_then,
            pack_message,
            brotli_compress,
        )),
        decode: Arc::new(crate::compose!(
            and_then,
            brotli_decompress,
            unpack_message,
        )),
    };

    pub static ref DEFAULT_UPLINK_CODEC: UplinkCodec = UplinkCodec {
        encode: Arc::new(pack_message),
        decode: Arc::new(unpack_message),
    };

    pub static ref DEFAULT_LINK_CODEC: LinkCodecs = LinkCodecs {
        uplink: Arc::new(DEFAULT_UPLINK_CODEC.clone()),
        downlink: Arc::new(DEFAULT_DOWNLINK_CODEC.clone()),
    };
}

#[cfg(feature = "serial_cobs")]
lazy_static::lazy_static! {
    pub static ref DEFAULT_SERIAL_CODEC: SerialCodec = SerialCodec {
        encode:   Arc::new(crate::compose!(
            map,
            pack_message,
            |v| pack_cobs(v, DEFAULT_COBS_SENTINEL)
        )),
        decode: Arc::new(crate::compose!(
            and_then,
            |v| unpack_cobs(v, DEFAULT_COBS_SENTINEL),
            |unpacked| {
                tracing::debug!(content = %hex::encode(&unpacked), "cobs serial message unpacked");
                Ok(unpacked)
            },
            unpack_message
        )),
        sentinel:    vec![DEFAULT_COBS_SENTINEL],
    };
}

#[cfg(not(feature = "serial_cobs"))]
lazy_static::lazy_static! {
    pub static ref DEFAULT_SERIAL_CODEC: SerialCodec = SerialCodec {
        encode:     Arc::new(pack_message),
        decode:     Arc::new(unpack_message),
        sentinel:   vec![crate::message::header::Magic::VALUE, crate::message::header::Destination::Frontend as u8],
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
#[tracing::instrument(err(Display), level = "trace")]
pub fn unpack_message<T>(v: Vec<u8>) -> eyre::Result<CRCMessage<T>>
where
    T: PackedStructSlice,
{
    <CRCMessage<T> as PackedStructSlice>::unpack_from_slice(&v).map_err(eyre::Report::from)
}

#[inline]
#[tracing::instrument(skip(t), err(Display), level = "trace")]
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

#[tracing::instrument(skip_all, level = "trace")]
pub fn brotli_decompress(v: Vec<u8>) -> eyre::Result<Vec<u8>> {
    let mut out = vec![];
    brotli::BrotliDecompress(&mut &v[..], &mut out).map_err(eyre::Report::from)?;

    Ok(out)
}

#[cfg(not(debug_assertions))]
const SEND_TIMEOUT: u64 = 1000;

#[cfg(any(test, debug_assertions))]
const SEND_TIMEOUT: u64 = 15000;

#[tracing::instrument(level = "debug", skip(backoff, csq), fields(%msg), err(Display), ret)]
#[inline]
pub async fn send(
    desc: &str,
    msg: CRCMessage<OpaqueBytes>,
    backoff: impl Backoff + Clone,
    csq: impl Borrow<CommandSequencer>,
) -> eyre::Result<CRCMessage<Ack>> {
    let csq = csq.borrow();

    let result = backoff::future::retry_notify(backoff, move || {
        let msg = msg.clone();

        async move {
            let result = timeout(SEND_TIMEOUT, csq.submit(msg))
                .await
                .and_then(|x| x)
                .map_err(backoff::Error::transient)?;

            if result.header.ty.request_was_invalid {
                return Err(backoff::Error::transient(eyre::eyre!("cs reports '{}' invalid", desc)));
            }

            Ok(result)
        }
    }, |e, dur| {
        tracing::warn!(error = %e, backoff_dur = ?dur, "submitting '{}' to central station", desc);
    }).await?;

    Ok(result)
}
