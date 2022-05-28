use crc::{
    Crc,
    CRC_8_SMBUS,
};
use lazy_static::lazy::Lazy;
use packed_struct::{
    prelude::*,
    PackingResult,
};

use crate::message::{
    HeaderPacket,
    Message,
    OpaqueBytes,
};

mod ack;
mod realtime_status;

pub use ack::Ack;
pub use realtime_status::RealtimeStatus;

pub type Relay = HeaderPacket<RealtimeStatus, Vec<Message<OpaqueBytes>>>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Payload<T>(T);

impl<T> Payload<T> {
    const CRC: Crc<u8> = Crc::<u8>::new(&CRC_8_SMBUS);

    #[inline]
    pub fn new(data: T) -> Self {
        Self(data)
    }

    #[inline]
    pub fn take(self) -> T {
        self.0
    }
}

impl<T> AsRef<T> for Payload<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> PackedStructSlice for Payload<T>
where
    T: PackedStructSlice,
{
    fn pack_to_slice(&self, output: &mut [u8]) -> PackingResult<()> {
        let size = Self::packed_bytes_size(Some(self))?;

        let (payload, &mut [ref mut checksum]) = output.split_at_mut(size - 1) else {
            return Err(PackingError::BufferTooSmall);
        };

        self.0.pack_to_slice(payload)?;
        *checksum = Self::CRC.checksum(payload);

        Ok(())
    }

    fn unpack_from_slice(src: &[u8]) -> PackingResult<Self> {
        let (payload, &[src_checksum]) = src.split_at(src.len() - 1) else {
            return Err(PackingError::BufferTooSmall);
        };

        let computed_checksum = Self::CRC.checksum(payload);

        if src_checksum != computed_checksum {
            tracing::error!(src_checksum, computed_checksum, "message with invalid checksum");
            return Err(PackingError::InvalidValue);
        }

        let payload = T::unpack_from_slice(payload)?;
        Ok(Self::new(payload))
    }

    fn packed_bytes_size(opt_self: Option<&Self>) -> PackingResult<usize> {
        let slf = opt_self.ok_or(PackingError::InstanceRequiredForSize)?;

        let count = T::packed_bytes_size(Some(&slf.0))? + 1;
        Ok(count)
    }
}
