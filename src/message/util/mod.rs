mod header_packet;
mod magic_value;

pub use header_packet::HeaderPacket;
pub use magic_value::MagicValue;

pub type OpaqueBytes = Vec<u8>;
