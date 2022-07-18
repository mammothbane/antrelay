#![feature(const_trait_impl)]
#![feature(const_convert)]

pub mod checksum;
pub mod crc;
pub mod header;
mod header_packet;
mod magic_value;
mod mission_epoch;
pub mod payload;

pub use checksum::Checksum;
pub use header::Header;
pub use header_packet::HeaderPacket;
pub use magic_value::MagicValue;
pub use mission_epoch::MissionEpoch;

pub type OpaqueBytes = Vec<u8>;

impl_checksum!(pub StandardCRC, u8, ::crc::CRC_8_SMBUS);

pub type WithCRC<T, CRC = StandardCRC> = crc::WithCRC<T, CRC>;
pub type Message<T, CRC = StandardCRC> = WithCRC<HeaderPacket<Header, T>, CRC>;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct UniqueId {
    timestamp: MissionEpoch,
    seq:       u8,
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct RTParams {
    pub(crate) time: MissionEpoch,
    pub(crate) seq:  u8,
}
