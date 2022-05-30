pub mod crc_wrap;
pub mod header;
mod util;

pub use header::Header;
pub use util::*;

crate::impl_checksum!(pub StandardCRC, u8, crc::CRC_8_SMBUS);

pub type CRCWrap<T, CRC = StandardCRC> = crc_wrap::CRCWrap<T, CRC>;
pub type Message<T, CRC = StandardCRC> = HeaderPacket<Header, CRCWrap<T, CRC>>;

#[derive(
    Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
pub struct UniqueId {
    timestamp: crate::MissionEpoch,
    seq:       u8,
}
