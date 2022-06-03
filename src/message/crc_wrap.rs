use std::{
    fmt::Debug,
    marker::PhantomData,
};

use once_cell::sync::OnceCell;

use packed_struct::{
    prelude::*,
    PackingResult,
};

use crate::message::{
    checksum,
    payload::realtime_status::RealtimeStatus,
    Checksum,
    HeaderPacket,
    Message,
    OpaqueBytes,
};

pub type Relay = HeaderPacket<RealtimeStatus, Vec<Message<OpaqueBytes>>>;

#[derive(Debug, Clone, PartialEq, derive_more::Display)]
#[display(fmt = "{:?}", val)]
pub struct CRCWrap<T, CRC> {
    val:            T,
    cache_bytes:    OnceCell<PackingResult<Vec<u8>>>,
    cache_checksum: OnceCell<PackingResult<checksum::Array>>,
    _phantom:       PhantomData<CRC>,
}

impl<T, CRC> CRCWrap<T, CRC> {
    #[inline]
    pub fn new(data: T) -> Self {
        Self {
            val:            data,
            cache_bytes:    OnceCell::new(),
            cache_checksum: OnceCell::new(),
            _phantom:       PhantomData,
        }
    }

    #[inline]
    pub fn take(self) -> T {
        self.val
    }
}

impl<T, CRC> CRCWrap<T, CRC>
where
    CRC: Checksum,
{
    pub const CRC_SIZE: usize = checksum::size::<CRC>();

    #[inline]
    pub fn payload_bytes(&self) -> PackingResult<&[u8]>
    where
        T: PackedStructSlice,
    {
        self.cache_bytes
            .get_or_init(|| self.val.pack_to_vec())
            .as_ref()
            .map_err(|e| *e)
            .map(|v| v.as_slice())
    }

    #[inline]
    pub fn checksum(&self) -> PackingResult<&[u8]>
    where
        T: PackedStructSlice,
    {
        self.cache_checksum
            .get_or_init(|| {
                let payload = self.payload_bytes()?;

                Ok(CRC::checksum_array(payload))
            })
            .as_ref()
            .map(|array| array.as_slice())
            .map_err(|&e| e)
    }

    #[inline]
    fn split_point(buf: &[u8]) -> usize {
        buf.len().saturating_sub(Self::CRC_SIZE)
    }
}

impl<T, CRC> AsRef<T> for CRCWrap<T, CRC> {
    fn as_ref(&self) -> &T {
        &self.val
    }
}

impl<T, CRC> PackedStructSlice for CRCWrap<T, CRC>
where
    T: PackedStructSlice,
    CRC: Checksum,
{
    fn pack_to_slice(&self, output: &mut [u8]) -> PackingResult<()> {
        let size = Self::packed_bytes_size(Some(self))?;

        let split_point = Self::split_point(output);
        let (payload, checksum) = output[..size].split_at_mut(split_point);

        if checksum.len() != Self::CRC_SIZE {
            return Err(PackingError::BufferTooSmall);
        };

        payload.copy_from_slice(self.payload_bytes()?);
        checksum.copy_from_slice(self.checksum()?);

        Ok(())
    }

    fn unpack_from_slice(src: &[u8]) -> PackingResult<Self> {
        let (payload, src_checksum) = src.split_at(Self::split_point(src));

        if src_checksum.len() != Self::CRC_SIZE {
            return Err(PackingError::BufferTooSmall);
        }

        let payload = T::unpack_from_slice(payload)?;
        let result = Self::new(payload);

        let computed_checksum = result.checksum()?;

        if src_checksum != computed_checksum {
            tracing::error!(
                src_checksum = %hex::encode(src_checksum),
                computed_checksum = %hex::encode(computed_checksum),
                "message with invalid checksum"
            );

            return Err(PackingError::InvalidValue);
        }

        Ok(result)
    }

    fn packed_bytes_size(opt_self: Option<&Self>) -> PackingResult<usize> {
        let slf = opt_self.ok_or(PackingError::InstanceRequiredForSize)?;

        let count = T::packed_bytes_size(Some(&slf.val))? + Self::CRC_SIZE;
        Ok(count)
    }
}
