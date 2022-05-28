#![feature(never_type)]
#![feature(try_blocks)]
#![feature(const_option_ext)]
#![feature(option_result_contains)]
#![feature(const_trait_impl)]
#![feature(let_else)]

use chrono::{
    DateTime,
    Utc,
};

mod mission_epoch;

pub mod build;
pub mod message;
pub mod util;

pub use mission_epoch::{
    MissionEpoch,
    EPOCH,
};
