use crate::message::OpaqueBytes;
use packed_struct::{
    prelude::*,
    PackedStructInfo,
    PackingResult,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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

    pub fn payload_packed(&self) -> PackingResult<HeaderPacket<Header, OpaqueBytes>>
    where
        Payload: PackedStructSlice,
        Header: Clone,
    {
        Ok(HeaderPacket {
            payload: self.payload.pack_to_vec()?,
            header:  self.header.clone(),
        })
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
    fn pack_to_slice(&self, output: &mut [u8]) -> PackingResult<()> {
        let (header_bytes, payload_bytes) = output.split_at_mut(Self::header_bytes());

        self.header.pack_to_slice(header_bytes)?;
        self.payload.pack_to_slice(payload_bytes)?;

        Ok(())
    }

    fn unpack_from_slice(src: &[u8]) -> PackingResult<Self> {
        let (header_bytes, payload_bytes) = src.split_at(Self::header_bytes());

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
