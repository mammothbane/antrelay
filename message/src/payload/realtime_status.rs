use packed_struct::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PackedStruct)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "3", endian = "msb")]
pub struct RealtimeStatus {
    pub memory_usage: u8,
    pub logs_pending: u8,

    #[packed_field(size_bytes = "1", ty = "enum")]
    pub flags: Flags,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PrimitiveEnum_u8)]
pub enum Flags {
    None                = 0x0,
    Booted              = 0x01,
    Shutdown            = 0x02,
    SerialError         = 0x04,
    SocketError         = 0x08,
    DownlinkPacketError = 0x10,
    UplinkPacketError   = 0x20,
}

impl RealtimeStatus {
    pub const MEMORY_RANGE_MB: std::ops::Range<f64> = 0.0..64.0;
    const MEMORY_RANGE_SIZE: f64 = Self::MEMORY_RANGE_MB.end - Self::MEMORY_RANGE_MB.start;

    #[inline]
    pub fn memory_mb(&self) -> f64 {
        self.memory_usage as f64 * Self::MEMORY_RANGE_SIZE + Self::MEMORY_RANGE_MB.start
    }
}
