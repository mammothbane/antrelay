#![feature(never_type)]
#![feature(try_blocks)]
#![feature(const_option_ext)]
#![feature(option_result_contains)]
#![feature(const_trait_impl)]
#![feature(let_else)]
#![feature(associated_type_defaults)]
#![feature(iter_intersperse)]
#![feature(const_convert)]
#![feature(backtrace)]
#![deny(unsafe_code)]

mod mission_epoch;

pub mod build;
pub mod futures;
pub mod io;
mod macros;
pub mod message;
pub mod net;
pub mod relay;
pub mod signals;
pub mod standard_graph;
pub mod tracing;
pub mod util;

pub use mission_epoch::{
    MissionEpoch,
    EPOCH,
};
