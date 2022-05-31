#![feature(try_blocks)]
#![feature(never_type)]
#![feature(proc_macro_hygiene)]
#![feature(stmt_expr_attributes)]
#![feature(let_else)]
#![feature(explicit_generic_args_with_impl_trait)]
#![feature(iter_intersperse)]

extern crate core;

use eyre::Result;
use structopt::StructOpt as _;

use lunarrelay::{
    build,
    net::DEFAULT_BACKOFF,
    relay,
    signals,
    util,
};

pub use crate::options::Options;

pub mod trace;

mod options;

#[cfg(windows)]
type Socket = smol::net::UdpSocket;

#[cfg(unix)]
type Socket = smol::net::unix::UnixDatagram;

fn main() -> Result<()> {
    util::bootstrap!(
        "starting {} {} ({}, built at {} with rustc {})",
        build::PACKAGE,
        build::VERSION,
        build::COMMIT_HASH,
        build::BUILD_TIMESTAMP,
        build::RUSTC_COMMIT_HASH,
    );

    let options: Options = Options::from_args();

    let log_stream = trace::init()?;

    tracing::info!(
        downlink_ty = ?lunarrelay::message::payload::log::Type::Startup,
        application = build::PACKAGE,
        version = build::VERSION,
        build_commit = build::COMMIT_HASH,
        built_at = build::BUILD_TIMESTAMP,
        using_rustc = build::RUSTC_COMMIT_HASH,
        "tracing subsystem initialized"
    );

    let signal_done = signals::signals()?;

    smol::block_on({
        async move {
            #[cfg(unix)]
            util::dynload::apply_patches(&options.lib_dir).await;

            let (read, write) = relay::connect_serial(options.serial_port, options.baud).await?;

            let uplink = relay::uplink_stream::<Socket>(
                options.uplink_address.clone(),
                DEFAULT_BACKOFF.clone(),
                1024,
            )
            .await;

            let downlink_packets = relay::graph::<Socket>(
                signal_done,
                read,
                write,
                relay::SERIAL_REQUEST_BACKOFF.clone(),
                uplink,
                relay::dummy_log_downlink(log_stream),
            )
            .await;

            relay::send_downlink::<Socket>(
                downlink_packets,
                options.downlink_addresses.clone(),
                DEFAULT_BACKOFF.clone(),
            )
            .await;

            Ok(())
        }
    })
}
