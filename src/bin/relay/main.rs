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
        PacketEnv,
        DEFAULT_LINK_CODEC,
        DEFAULT_SERIAL_CODEC,
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
                SocketMode::Bind,
            )
            .pipe(stream_unwrap!("connecting to uplink socket"));

            let uplink = receive_packets(uplink_sockets).pipe(antrelay::trip!(tripwire));

            let downlink_sockets = options.downlink_addresses.iter().cloned().map(|addr| {
                net::socket_stream::<Socket>(addr, DEFAULT_BACKOFF.clone(), SocketMode::Connect)
                    .pipe(stream_unwrap!("connecting to downlink socket"))
            });

            let serial_codec = &*DEFAULT_SERIAL_CODEC;
            let packet_env = PacketEnv::default();
            let link_codec = &*DEFAULT_LINK_CODEC;

            let (serial_pump, csq, serial_downlink) = standard_graph::serial(
                (serial_codec, &packet_env),
                writer,
                reader,
                tripwire.clone(),
            );

            let graph = standard_graph::run(
                (link_codec, &packet_env),
                uplink,
                downlink_sockets,
                trace_event_stream.pipe(|s| dummy_log_downlink(&packet_env, s)),
                csq,
                serial_downlink,
                tripwire,
            );

            smol::future::zip(graph, serial_pump).await;

            Ok(())
        }
    })
}
