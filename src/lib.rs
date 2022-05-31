#![feature(never_type)]
#![feature(try_blocks)]
#![feature(const_option_ext)]
#![feature(option_result_contains)]
#![feature(const_trait_impl)]
#![feature(let_else)]
#![feature(associated_type_defaults)]
#![feature(explicit_generic_args_with_impl_trait)]
#![feature(iter_intersperse)]

mod mission_epoch;

pub mod build;
pub mod message;
pub mod net;
pub mod packet_io;
pub mod relay;
pub mod signals;
pub mod tracing;
pub mod util;

pub use mission_epoch::{
    MissionEpoch,
    EPOCH,
};
