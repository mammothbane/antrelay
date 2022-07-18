#![feature(associated_type_defaults)]

pub use datagram::{
    DatagramOps,
    DatagramReceiver,
    DatagramSender,
};

mod datagram;

#[cfg(unix)]
mod unix;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SocketMode {
    Connect,
    Bind,
}
