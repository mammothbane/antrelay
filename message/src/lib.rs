#![feature(const_trait_impl)]
#![feature(const_convert)]

use std::fmt::{
    Display,
    Formatter,
};

use tap::Conv;

mod bytes_wrap;
pub mod checksum;
pub mod crc;
mod downlink;
pub mod header;
mod header_packet;
mod magic_value;
mod mission_epoch;
pub mod payload;
pub mod source_info;

pub use bytes_wrap::BytesWrap;
pub use checksum::Checksum;
pub use downlink::Downlink;
pub use header::Header;
pub use header_packet::HeaderPacket;
pub use magic_value::MagicValue;
pub use mission_epoch::MissionEpoch;
pub use source_info::SourceInfo;

use crate::header::{
    Destination,
    Event,
};

impl_checksum!(pub StandardCRC, u8, ::crc::CRC_8_SMBUS);

pub type HeaderWithSource = HeaderPacket<Header, SourceInfo>;

pub type WithCRC<T, CRC = StandardCRC> = crc::WithCRC<T, CRC>;
pub type Message<T = BytesWrap, CRC = StandardCRC> =
    WithCRC<HeaderPacket<HeaderWithSource, T>, CRC>;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct UniqueId {
    timestamp: MissionEpoch,
    seq:       u8,
}

impl Display for UniqueId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} [seq {}]", self.timestamp.conv::<chrono::DateTime<chrono::Utc>>(), self.seq)
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct Params {
    pub time: MissionEpoch,
    pub seq:  u8,
}

#[inline]
pub fn command(env: &Params, dest: Destination, event: Event) -> Message<BytesWrap, StandardCRC> {
    Message::new(HeaderPacket {
        header:  HeaderPacket {
            header:  Header::command(env, dest, event),
            payload: SourceInfo::Empty,
        },
        payload: BytesWrap::from(&[]),
    })
}

#[inline]
pub fn downlink(env: &Params, b: impl AsRef<[u8]>) -> Message<BytesWrap, StandardCRC> {
    Message::new(HeaderPacket {
        header:  HeaderPacket {
            header:  Header::downlink(env, Event::FEPing),
            payload: SourceInfo::Empty,
        },
        payload: BytesWrap::from(b.as_ref()),
    })
}
