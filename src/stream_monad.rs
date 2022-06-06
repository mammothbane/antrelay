use std::{
    marker::PhantomData,
    pin::Pin,
};

use async_std::prelude::Stream;
use packed_struct::PackedStructSlice;
use smol::stream::StreamExt;
use tap::Pipe;

pub trait StreamCodec {
    type Output;
    type Input;

    type Error;

    fn encode<'a, S>(
        s: S,
    ) -> Pin<Box<dyn Stream<Item = Result<Self::Output, Self::Error>> + 'a + Send>>
    where
        S: Stream<Item = Self::Input> + 'a + Send,
        S::Item: 'a;
}

pub trait StreamExt: Stream {}

pub trait InvertibleCodec: StreamCodec {
    type Invert: StreamCodec<Output = Self::Input, Input = Self::Output>;
}

pub struct Identity<T>(PhantomData<T>);

impl<T> StreamCodec for Identity<T> {
    type Error = !;
    type Input = T;
    type Output = T;

    #[inline]
    fn encode<'a, S>(s: S) -> Pin<Box<dyn Stream<Item = Result<T, !>> + 'a + Send>>
    where
        S: Stream<Item = T> + 'a + Send,
        S::Item: 'a,
    {
        Box::pin(s.map(Ok))
    }
}

impl<T> InvertibleCodec for Identity<T> {
    type Invert = Self;
}

pub trait BrotliParams {
    fn get() -> &'static brotli::enc::BrotliEncoderParams;
}

pub struct BrotliCompress<T>(PhantomData<T>);

impl<T> StreamCodec for BrotliCompress<T>
where
    T: BrotliParams,
{
    type Error = std::io::Error;
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn encode<'a, S>(
        s: S,
    ) -> Pin<Box<dyn Stream<Item = Result<Self::Output, Self::Error>> + 'a + Send>>
    where
        S: Stream<Item = Self::Input> + 'a + Send,
    {
        Box::pin(s.map(|v| {
            let mut out = vec![];

            let count = brotli::BrotliCompress(&mut &v[..], &mut out, T::get());
            count.map(move |_| out)
        }))
    }
}

pub struct BrotliDecompress<T = !>(PhantomData<T>);

impl<T> StreamCodec for BrotliDecompress<T> {
    type Error = std::io::Error;
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn encode<'a, S>(
        s: S,
    ) -> Pin<Box<dyn Stream<Item = Result<Self::Output, Self::Error>> + 'a + Send>>
    where
        S: Stream<Item = Self::Input> + 'a + Send,
    {
        Box::pin(s.map(|v| {
            let mut out = vec![];

            let count = brotli::BrotliDecompress(&mut &v[..], &mut out);
            count.map(move |_| out)
        }))
    }
}

impl<T> InvertibleCodec for BrotliDecompress<T>
where
    T: BrotliParams,
{
    type Invert = BrotliCompress<T>;
}

pub struct Compose<T, U>(PhantomData<(T, U)>);

impl<B, T, U> StreamCodec for Compose<T, U>
where
    T: StreamCodec<Output = B, Error = !>,
    U: StreamCodec<Input = B>,
    B: 'static,
    T::Input: 'static,
{
    type Error = U::Error;
    type Input = T::Input;
    type Output = U::Output;

    fn encode<'a, S>(
        s: S,
    ) -> Pin<Box<dyn Stream<Item = Result<Self::Output, Self::Error>> + 'a + Send>>
    where
        S: Stream<Item = Self::Input> + 'a + Send,
    {
        s.pipe(T::encode).map(|x| x.unwrap()).pipe(U::encode)
    }
}

pub struct SlicePack<T>(PhantomData<T>);

pub struct SliceUnpack<T>(PhantomData<T>);

impl<T> StreamCodec for SlicePack<T>
where
    T: PackedStructSlice,
{
    type Error = packed_struct::PackingError;
    type Input = T;
    type Output = Vec<u8>;

    fn encode<'a, S>(
        s: S,
    ) -> Pin<Box<dyn Stream<Item = Result<Self::Output, Self::Error>> + 'a + Send>>
    where
        S: Stream<Item = Self::Input> + 'a + Send,
        S::Item: 'a,
    {
        Box::pin(s.map(|elt| elt.pack_to_vec()))
    }
}

impl<T> StreamCodec for SliceUnpack<T>
where
    T: PackedStructSlice,
{
    type Error = packed_struct::PackingError;
    type Input = Vec<u8>;
    type Output = T;

    fn encode<'a, S>(
        s: S,
    ) -> Pin<Box<dyn Stream<Item = Result<Self::Output, Self::Error>> + 'a + Send>>
    where
        S: Stream<Item = Self::Input> + 'a + Send,
        S::Item: 'a,
    {
        Box::pin(s.map(|elt| T::unpack_from_slice(&elt)))
    }
}
