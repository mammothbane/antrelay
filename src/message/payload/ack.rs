use packed_struct::prelude::*;

use crate::{
    message,
    message::UniqueId,
    MissionEpoch,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PackedStruct)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "6", endian = "msb")]
pub struct Ack {
    #[packed_field(size_bytes = "4")]
    pub timestamp: MissionEpoch,
    pub seq:       u8,
    pub checksum:  u8,
}

impl Ack {
    #[inline]
    pub fn matches(&self, header: &message::Header) -> bool {
        self.unique_id() == header.unique_id()
    }

    #[inline]
    pub fn unique_id(&self) -> UniqueId {
        UniqueId {
            timestamp: self.timestamp,
            seq:       self.seq,
        }
    }
}
