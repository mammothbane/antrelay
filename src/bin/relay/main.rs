#![feature(try_blocks)]
#![feature(never_type)]
#![feature(proc_macro_hygiene)]
#![feature(stmt_expr_attributes)]
#![feature(let_else)]
#![feature(iter_intersperse)]
#![deny(unsafe_code)]

use eyre::Result;
use structopt::StructOpt as _;
use tap::Pipe;

use util::build;

pub use crate::options::Options;

pub mod trace;

mod options;

#[cfg(windows)]
type Socket = tokio::net::UdpSocket;

#[cfg(unix)]
type Socket = tokio::net::unix::UnixDatagram;

#[actix::main]
async fn main() -> Result<()> {
    util::bootstrap!(
        "starting {} {} ({}, built at {} with rustc {})",
        build::PACKAGE,
        build::VERSION,
        build::COMMIT_HASH,
        build::BUILD_TIMESTAMP,
        build::RUSTC_COMMIT_HASH,
    );

    let options: Options = Options::from_args();

    trace::init();

    tracing::info!(
        application = %build::PACKAGE,
        version = %build::VERSION,
        build_commit = %build::COMMIT_HASH,
        built_at = %build::BUILD_TIMESTAMP,
        using_rustc = %build::RUSTC_COMMIT_HASH,
        "tracing subsystem initialized"
    );

    Ok(())
}
