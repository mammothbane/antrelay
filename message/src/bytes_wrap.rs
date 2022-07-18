use bytes::{
    Bytes,
    BytesMut,
};
use packed_struct::{
    PackedStructSlice,
    PackingError,
    PackingResult,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, derive_more::Into, derive_more::AsRef)]
pub struct BytesWrap(Bytes);

impl PackedStructSlice for BytesWrap {
    fn pack_to_slice(&self, output: &mut [u8]) -> PackingResult<()> {
        output.copy_from_slice(self.0.as_ref());
        Ok(())
    }

    fn unpack_from_slice(src: &[u8]) -> PackingResult<Self> {
        Ok(Self(BytesMut::from(src).freeze()))
    }

    fn packed_bytes_size(opt_self: Option<&Self>) -> PackingResult<usize> {
        let slf = opt_self.ok_or(PackingError::InstanceRequiredForSize)?;

        Ok(slf.0.len())
    }
}

impl<T> From<T> for BytesWrap
where
    T: AsRef<[u8]>,
{
    fn from(t: T) -> Self {
        let mut m = BytesMut::new();
        m.copy_from_slice(t.as_ref());

        BytesWrap(m.freeze())
    }
}
