use crc::Crc;
use packed_struct::{
    prelude::*,
    PackingResult,
};

pub mod header;
pub mod payload;
mod util;

pub use header::Header;
pub use payload::Payload;
pub use util::*;

crate::impl_checksum!(pub StandardCRC, u8, crc::CRC_8_SMBUS);

pub type Message<T> = HeaderPacket<Header, Payload<T, StandardCRC>>;
