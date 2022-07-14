use std::marker::PhantomData;

use bytes::BytesMut;
use packed_struct::PackedStructSlice;
use tokio_util::codec::{
    Decoder,
    Encoder,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    PackingError(#[from] packed_struct::PackingError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct PackedSliceCodec<T>(PhantomData<T>);

impl<T> Encoder<T> for PackedSliceCodec<T>
where
    T: PackedStructSlice,
{
    type Error = Error;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        item.pack_to_slice(dst.as_mut())?;

        Ok(())
    }
}

impl<T> Decoder for PackedSliceCodec<T>
where
    T: PackedStructSlice,
{
    type Error = Error;
    type Item = T;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let result = T::unpack_from_slice(src.as_ref())?;
        Ok(Some(result))
    }
}
