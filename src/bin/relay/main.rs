#![feature(try_blocks)]
#![feature(never_type)]
#![feature(proc_macro_hygiene)]
#![feature(stmt_expr_attributes)]
#![feature(let_else)]
#![feature(explicit_generic_args_with_impl_trait)]
#![feature(iter_intersperse)]
#![deny(unsafe_code)]

use eyre::Result;
use smol::stream::StreamExt as _;
use structopt::StructOpt as _;
use tap::Pipe;

use antrelay::{
    build,
    compose,
    io::{
        pack_cobs,
        unpack_cobs,
    },
    net,
    net::{
        receive_packets,
        SocketMode,
        DEFAULT_BACKOFF,
    },
    relay,
    relay::dummy_log_downlink,
    signals,
    standard_graph,
    stream_unwrap,
    util::{
        brotli_compress,
        brotli_decompress,
        pack_message,
        unpack_message,
    },
};

pub use crate::options::Options;

pub mod trace;

mod options;

#[cfg(windows)]
type Socket = smol::net::UdpSocket;

#[cfg(unix)]
type Socket = smol::net::unix::UnixDatagram;

fn main() -> Result<()> {
    antrelay::bootstrap!(
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
        downlink_ty = ?antrelay::message::payload::log::Type::Startup,
        application = build::PACKAGE,
        version = build::VERSION,
        build_commit = build::COMMIT_HASH,
        built_at = build::BUILD_TIMESTAMP,
        using_rustc = build::RUSTC_COMMIT_HASH,
        "tracing subsystem initialized"
    );

    let signal_done = signals::signals()?;

    let (trigger, tripwire) = smol::channel::bounded::<!>(1);

    smol::spawn(async move {
        signal_done.recv().await.unwrap_err();
        trigger.close();
    })
    .detach();

    smol::block_on({
        async move {
            #[cfg(unix)]
            unsafe {
                antrelay::util::dynload::apply_patches(&options.lib_dir).await;
            }

            let (reader, writer) = relay::connect_serial(options.serial_port, options.baud).await?;

            let uplink_sockets = net::socket_stream::<Socket>(
                options.uplink_address,
                DEFAULT_BACKOFF.clone(),
                SocketMode::Connect,
            )
            .pipe(stream_unwrap!("connecting to uplink socket"));

            let uplink = receive_packets(uplink_sockets).pipe(antrelay::trip!(tripwire));

            let downlink_sockets = options.downlink_addresses.iter().cloned().map(|addr| {
                net::socket_stream::<Socket>(addr, DEFAULT_BACKOFF.clone(), SocketMode::Connect)
                    .pipe(stream_unwrap!("connecting to downlink socket"))
            });

            let (serial_pump, csq, serial_downlink) = standard_graph::serial::<chrono::Utc>(
                writer,
                compose!(map, pack_message, |v| pack_cobs(v, 0u8)),
                reader,
                compose!(and_then, |v| unpack_cobs(v, 0u8), unpack_message),
                tripwire.clone(),
            );

            let graph = standard_graph::run(
                uplink,
                compose!(and_then, brotli_decompress, unpack_message),
                downlink_sockets,
                compose!(and_then, pack_message, |v| Ok(pack_cobs(v, 0u8)), brotli_compress),
                trace_event_stream.pipe(dummy_log_downlink),
                csq,
                serial_downlink,
                tripwire,
            );

            smol::future::zip(graph, serial_pump).await;

            Ok(())
        }
    })
}
