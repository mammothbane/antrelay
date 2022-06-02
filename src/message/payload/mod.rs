use crate::message::{
    crc_wrap::RealtimeStatus,
    HeaderPacket,
    OpaqueBytes,
};

mod ack;
pub mod log;
pub mod realtime_status;

pub use ack::Ack;

pub type RelayPacket = HeaderPacket<RealtimeStatus, OpaqueBytes>;
