use crate::{
    BytesWrap,
    HeaderPacket,
};

mod ack;
pub mod log;
pub mod realtime_status;

pub use ack::Ack;
pub use realtime_status::RealtimeStatus;

pub type RelayPacket = HeaderPacket<RealtimeStatus, BytesWrap>;
