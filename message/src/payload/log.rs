use packed_struct::prelude::*;

use crate::{
    HeaderPacket,
    MissionEpoch,
};

pub type Log = HeaderPacket<Header, Box<dyn PackedStructSlice>>;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PackedStruct)]
pub struct Header {
    #[packed_field(size_bytes = "4")]
    pub timestamp: MissionEpoch,
    #[packed_field(size_bytes = "1", ty = "enum")]
    pub ty:        Type,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PrimitiveEnum_u8)]
pub enum Type {
    Startup,
    DownlinkConnected,
    UplinkConnected,
    SerialConnected,
    RelayStarted,

    Interrupted,
    Shutdown,
}
