use crate::{
    BytesWrap,
    HeaderPacket,
};

pub mod log;
pub mod realtime_status;

pub use realtime_status::RealtimeStatus;

pub type RelayPacket = HeaderPacket<RealtimeStatus, BytesWrap>;
