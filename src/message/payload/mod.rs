use std::marker::PhantomData;

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
    util::{
        checksum,
        Checksum,
    },
    HeaderPacket,
    Message,
    OpaqueBytes,
};

mod ack;
mod log;
mod realtime_status;

pub use ack::Ack;
pub use realtime_status::RealtimeStatus;

pub type Relay = HeaderPacket<RealtimeStatus, Vec<Message<OpaqueBytes>>>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Payload<T, CRC>(T, PhantomData<CRC>);

impl<T, CRC> Payload<T, CRC> {
    #[inline]
    pub fn new(data: T) -> Self {
        Self(data, PhantomData)
    }

    #[inline]
    pub fn take(self) -> T {
        self.0
    }
}

impl<T, CRC> Payload<T, CRC>
where
    CRC: Checksum,
{
    pub const CRC_SIZE: usize = checksum::size::<CRC>();

    #[inline]
    fn split_point(buf: &[u8]) -> usize {
        buf.len().saturating_sub(Self::CRC_SIZE)
    }
}

impl<T, CRC> AsRef<T> for Payload<T, CRC> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T, CRC> PackedStructSlice for Payload<T, CRC>
where
    T: PackedStructSlice,
    CRC: Checksum,
    CRC::Output: PackedStructSlice,
{
    fn pack_to_slice(&self, output: &mut [u8]) -> PackingResult<()> {
        let size = Self::packed_bytes_size(Some(self))?;

        let split_point = Self::split_point(output);
        let (payload, checksum) = output[..size].split_at_mut(split_point);

        if checksum.len() != Self::CRC_SIZE {
            return Err(PackingError::BufferTooSmall);
        };

        self.0.pack_to_slice(payload)?;

        let computed_checksum = CRC::checksum_array(payload);
        checksum.copy_from_slice(&computed_checksum[..]);

        Ok(())
    }

    fn unpack_from_slice(src: &[u8]) -> PackingResult<Self> {
        let (payload, src_checksum) = src.split_at(Self::split_point(src));

        if src_checksum.len() != Self::CRC_SIZE {
            return Err(PackingError::BufferTooSmall);
        }

        let computed_checksum = CRC::checksum_array(payload);

        if src_checksum != &computed_checksum[..] {
            tracing::error!(
                src_checksum = %hex::encode(src_checksum),
                computed_checksum = %hex::encode(computed_checksum),
                "message with invalid checksum"
            );
            return Err(PackingError::InvalidValue);
        }

        let payload = T::unpack_from_slice(payload)?;
        Ok(Self::new(payload))
    }

    fn packed_bytes_size(opt_self: Option<&Self>) -> PackingResult<usize> {
        let slf = opt_self.ok_or(PackingError::InstanceRequiredForSize)?;

        let count = T::packed_bytes_size(Some(&slf.0))? + Self::CRC_SIZE;
        Ok(count)
    }
}
