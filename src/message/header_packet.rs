use packed_struct::{
    prelude::*,
    PackedStructInfo,
    PackingResult,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash, derive_more::Display)]
#[display(fmt = "{}/{:?}", header, payload)]
pub struct HeaderPacket<Header, Payload> {
    pub header:  Header,
    pub payload: Payload,
}

impl<Header, Payload> HeaderPacket<Header, Payload>
where
    Header: PackedStructInfo,
{
    #[inline]
    pub fn header_bytes() -> usize {
        let bits = Header::packed_bits();
        debug_assert_eq!(bits % 8, 0);

        bits / 8
    }

    pub fn payload_into<T>(&self) -> PackingResult<HeaderPacket<Header, T>>
    where
        Payload: PackedStructSlice,
        Header: Clone,
        T: PackedStructSlice,
    {
        let packed_payload = self.payload.pack_to_vec()?;
        let new_payload = T::unpack_from_slice(&packed_payload)?;

        Ok(HeaderPacket {
            header:  self.header.clone(),
            payload: new_payload,
        })
    }
}

impl<Header, Payload> PackedStructSlice for HeaderPacket<Header, Payload>
where
    Header: PackedStructInfo + PackedStructSlice,
    Payload: PackedStructSlice,
{
    #[tracing::instrument(fields(output.len = output.len()), skip(self, output), err, level = "trace")]
    fn pack_to_slice(&self, output: &mut [u8]) -> PackingResult<()> {
        let header_len = Self::header_bytes();
        if header_len > output.len() {
            return Err(PackingError::BufferTooSmall);
        }

        let (header_bytes, payload_bytes) = output.split_at_mut(header_len);

        self.header.pack_to_slice(header_bytes)?;
        self.payload.pack_to_slice(payload_bytes)?;

        Ok(())
    }

    #[tracing::instrument(skip(src), fields(src.len = src.len()), err, level = "trace")]
    fn unpack_from_slice(src: &[u8]) -> PackingResult<Self> {
        let header_len = Self::header_bytes();
        if header_len > src.len() {
            return Err(PackingError::BufferTooSmall);
        }

        let (header_bytes, payload_bytes) = src.split_at(header_len);

        let header = Header::unpack_from_slice(header_bytes)?;
        let payload = Payload::unpack_from_slice(payload_bytes)?;

        Ok(Self {
            header,
            payload,
        })
    }

    fn packed_bytes_size(opt_self: Option<&Self>) -> PackingResult<usize> {
        let payload_size = Payload::packed_bytes_size(opt_self.map(
            |&HeaderPacket {
                 ref payload,
                 ..
             }| payload,
        ))?;

        Ok(Self::header_bytes() + payload_size)
    }
}
