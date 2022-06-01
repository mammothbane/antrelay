#![feature(try_blocks)]
#![feature(never_type)]
#![feature(proc_macro_hygiene)]
#![feature(stmt_expr_attributes)]
#![feature(let_else)]
#![feature(explicit_generic_args_with_impl_trait)]
#![feature(iter_intersperse)]

extern crate core;

use eyre::Result;
use smol::stream::StreamExt;
use std::sync::Arc;
use structopt::StructOpt as _;

use lunarrelay::{
    build,
    net::DEFAULT_BACKOFF,
    packet_io::PacketIO,
    relay,
    signals,
    util,
    util::splittable_stream,
};
use stream_cancel::StreamExt as _;
use tap::Pipe;

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

    let trace_event_stream = trace::init()?;

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

            let (trigger, tripwire) = stream_cancel::Tripwire::new();

            smol::spawn(async move {
                signal_done.recv().await.unwrap_err();
                trigger.cancel()
            })
            .detach();

            let uplink = relay::uplink_stream::<Socket>(
                options.uplink_address.clone(),
                DEFAULT_BACKOFF.clone(),
                1024,
            )
            .await
            .take_until_if(tripwire.clone())
            .pipe(|s| splittable_stream(s, 1024));

            let packet_io = Arc::new(PacketIO::new(smol::io::BufReader::new(read), write));

            let all_serial = {
                let packet_io = packet_io.clone();
                packet_io.read_packets(0u8).await
            }
            .take_until_if(tripwire.clone());

            let serial_acks = relay::relay_uplink_to_serial(
                uplink.clone(),
                packet_io,
                relay::SERIAL_REQUEST_BACKOFF.clone(),
            )
            .await
            .for_each(|_| {});

            let downlink_packets = relay::assemble_downlink::<Socket>(
                uplink.clone(),
                all_serial,
                relay::dummy_log_downlink(trace_event_stream).take_until_if(tripwire.clone()),
            )
            .await;

            let downlink_future = relay::send_downlink::<Socket>(
                downlink_packets,
                options.downlink_addresses.clone(),
                DEFAULT_BACKOFF.clone(),
            );

            smol::future::zip(downlink_future, serial_acks).await;

            Ok(())
        }
    })
}
