pub mod checksum;
pub mod crc_wrap;
pub mod header;
mod header_packet;
mod magic_value;
pub mod payload;

pub use checksum::Checksum;
pub use header::Header;
pub use header_packet::HeaderPacket;
pub use magic_value::MagicValue;

pub type OpaqueBytes = Vec<u8>;

crate::impl_checksum!(pub StandardCRC, u8, crc::CRC_8_SMBUS);

pub type CRCWrap<T, CRC = StandardCRC> = crc_wrap::CRCWrap<T, CRC>;
pub type CRCMessage<T, CRC = StandardCRC> = HeaderPacket<Header, CRCWrap<T, CRC>>;

#[derive(
    Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
pub struct UniqueId {
    timestamp: crate::MissionEpoch,
    seq:       u8,
}
