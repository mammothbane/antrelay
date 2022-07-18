#![feature(const_trait_impl)]
#![feature(const_convert)]

mod bytes_wrap;
pub mod checksum;
pub mod crc;
pub mod header;
mod header_packet;
mod magic_value;
mod mission_epoch;
pub mod payload;

pub use bytes_wrap::BytesWrap;
pub use checksum::Checksum;
pub use header::Header;
pub use header_packet::HeaderPacket;
pub use magic_value::MagicValue;
pub use mission_epoch::MissionEpoch;

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
    pub time: MissionEpoch,
    pub seq:  u8,
}

#[inline]
pub fn new<T>(header: Header, t: T) -> Message<T, StandardCRC> {
    Message::new(HeaderPacket {
        header,
        payload: t,
    })
}
