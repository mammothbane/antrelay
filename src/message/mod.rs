use std::collections::HashMap;

use packed_struct::{
    prelude::*,
    PackedStructInfo,
    PackingResult,
};

mod header;
mod payload;

pub use header::{
    Destination,
    Header,
    Kind,
    Target,
    Type,
};
pub use payload::Payload;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Message {
    pub header:  Header,
    pub payload: Payload,
}

impl PackedStructSlice for Message {
    fn pack_to_slice(&self, output: &mut [u8]) -> PackingResult<()> {
        use packed_struct::PackedStruct;

        let (header_bytes, payload_bytes) = output.split_at_mut(*header::SIZE_BYTES);

        self.header.pack_to_slice(header_bytes)?;
        self.payload.pack_to_slice(payload_bytes)?;

        Ok(())
    }

    fn unpack_from_slice(src: &[u8]) -> PackingResult<Self> {
        let (header_bytes, payload_bytes) = src.split_at(*header::SIZE_BYTES);

        let header = Header::unpack_from_slice(header_bytes)?;
        let payload = Payload::unpack_from_slice(payload_bytes)?;

        Ok(Self {
            header,
            payload,
        })
    }

    fn packed_bytes_size(opt_self: Option<&Self>) -> PackingResult<usize> {
        let payload_size = Payload::packed_bytes_size(opt_self.map(|slf| &slf.payload))?;

        Ok(*header::SIZE_BYTES + payload_size)
    }
}
