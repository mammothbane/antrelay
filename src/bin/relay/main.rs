#![feature(try_blocks)]
#![feature(never_type)]
#![feature(proc_macro_hygiene)]
#![feature(stmt_expr_attributes)]
#![feature(let_else)]
#![feature(explicit_generic_args_with_impl_trait)]
#![feature(iter_intersperse)]
#![deny(unsafe_code)]

use eyre::Result;
use packed_struct::PackedStructSlice;
use smol::stream::StreamExt as _;
use structopt::StructOpt as _;
use tap::Pipe;

use lunarrelay::{
    build,
    net,
    net::{
        SocketMode,
        DEFAULT_BACKOFF,
    },
    relay,
    relay::dummy_log_downlink,
    signals,
    standard_graph,
    stream_unwrap,
};

pub use crate::options::Options;

pub mod trace;

mod options;

#[cfg(windows)]
type Socket = smol::net::UdpSocket;

#[cfg(unix)]
type Socket = smol::net::unix::UnixDatagram;

fn main() -> Result<()> {
    lunarrelay::bootstrap!(
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
            lunarrelay::util::dynload::apply_patches(&options.lib_dir).await;

            let (reader, writer) = relay::connect_serial(options.serial_port, options.baud).await?;

            let (trigger, tripwire) = smol::channel::bounded::<!>(1);

            smol::spawn(async move {
                signal_done.recv().await.unwrap_err();
                trigger.close();
            })
            .detach();

            let uplink_sockets = net::socket_stream::<Socket>(
                options.uplink_address,
                DEFAULT_BACKOFF.clone(),
                SocketMode::Connect,
            )
            .pipe(stream_unwrap!("connecting to uplink socket"));

            let uplink = relay::uplink_stream(uplink_sockets)
                .await
                .pipe(lunarrelay::trip!(tripwire))
                .map(|msg| msg.pack_to_vec())
                .pipe(stream_unwrap!("unpacking message"));

            let downlink_sockets = options.downlink_addresses.iter().cloned().map(|addr| {
                net::socket_stream::<Socket>(addr, DEFAULT_BACKOFF.clone(), SocketMode::Connect)
                    .pipe(stream_unwrap!("connecting to downlink socket"))
            });

            standard_graph::run(
                reader,
                writer,
                uplink,
                dummy_log_downlink(trace_event_stream),
                downlink_sockets,
                tripwire,
            )
            .0
            .await;

            Ok(())
        }
    })
}
