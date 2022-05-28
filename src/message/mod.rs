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

pub type Message<T> = HeaderPacket<Header, Payload<T>>;
