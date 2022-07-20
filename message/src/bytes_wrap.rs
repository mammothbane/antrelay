use bytes::{
    Bytes,
    BytesMut,
};
use packed_struct::{
    PackedStructSlice,
    PackingError,
    PackingResult,
};
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, derive_more::Into, derive_more::AsRef,
)]
pub struct BytesWrap(Bytes);

impl PackedStructSlice for BytesWrap {
    fn pack_to_slice(&self, output: &mut [u8]) -> PackingResult<()> {
        output.copy_from_slice(self.0.as_ref());
        Ok(())
    }

    fn unpack_from_slice(src: &[u8]) -> PackingResult<Self> {
        let mut b = BytesMut::with_capacity(src.len());
        b.extend_from_slice(src);

        Ok(Self(b.freeze()))
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
        m.extend_from_slice(t.as_ref());

        BytesWrap(m.freeze())
    }
}

impl Serialize for BytesWrap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BytesWrap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Vec::<u8>::deserialize(deserializer)?;
        let mut b = BytesMut::with_capacity(v.len());
        b.extend_from_slice(&v[..]);

        Ok(BytesWrap(b.freeze()))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use bytes::BytesMut;

    #[test]
    fn test_serde() {
        let b = BytesWrap(BytesMut::from(&[1, 2, 3, 4][..]).freeze());

        let result = serde_json::to_string(&b).unwrap();
        assert_eq!("[1,2,3,4]", result);

        let new: BytesWrap = serde_json::from_str(&result).unwrap();
        assert_eq!(b, new);
    }
}
