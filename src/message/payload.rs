use std::fmt::{
    Error,
    Formatter,
};

use packed_struct::{
    PackedStruct,
    PackedStructSlice,
    PackingError,
    PackingResult,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Payload {
    pub payload:  Vec<u8>,
    pub checksum: u8,
}

impl Payload {
    #[inline]
    pub fn new(data: Vec<u8>) -> Self {
        let checksum: u8 = todo!();

        Self {
            payload: data,
            checksum,
        }
    }

    #[inline]
    pub fn bytes_size(&self) -> usize {
        self.payload.len() + 1
    }
}

impl PackedStructSlice for Payload {
    fn pack_to_slice(&self, output: &mut [u8]) -> PackingResult<()> {
        let size = self.bytes_size();

        if output.len() < size {
            return Err(PackingError::BufferTooSmall);
        }

        output[..size - 1].copy_from_slice(&self.payload);
        output[size] = self.checksum;

        Ok(())
    }

    fn unpack_from_slice(src: &[u8]) -> PackingResult<Self> {
        if src.is_empty() {
            return Err(PackingError::BufferTooSmall);
        }

        let (payload, chk) = src.split_at(src.len() - 1);
        debug_assert_eq!(chk.len(), 1);

        Ok(Self {
            payload:  payload.to_vec(),
            checksum: *chk.first().unwrap(),
        })
    }

    fn packed_bytes_size(opt_self: Option<&Self>) -> PackingResult<usize> {
        opt_self.map(|s| s.bytes_size()).ok_or(PackingError::InstanceRequiredForSize)
    }
}
