#![feature(try_blocks)]
#![feature(never_type)]
#![feature(proc_macro_hygiene)]
#![feature(stmt_expr_attributes)]
#![feature(let_else)]
#![feature(explicit_generic_args_with_impl_trait)]
#![feature(iter_intersperse)]
#![deny(unsafe_code)]

use chrono::Utc;
use eyre::Result;
use hlist::{
    Find,
    HList,
};
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
        Clock,
        GroundLinkCodec,
        Seq,
        SerialCodec,
        U8Sequence,
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

            const SERIAL_SENTINEL: u8 = 0;

            let env = hlist::Nil
                .push(SerialCodec {
                    serialize:   Box::new(compose!(map, pack_message, |v| pack_cobs(
                        v,
                        SERIAL_SENTINEL
                    ))),
                    deserialize: Box::new(compose!(
                        and_then,
                        |v| unpack_cobs(v, SERIAL_SENTINEL),
                        unpack_message
                    )),
                    sentinel:    SERIAL_SENTINEL,
                })
                .push(GroundLinkCodec {
                    serialize:   Box::new(compose!(
                        and_then,
                        pack_message,
                        |v| Ok(pack_cobs(v, SERIAL_SENTINEL)),
                        brotli_compress
                    )),
                    deserialize: Box::new(compose!(and_then, brotli_decompress, unpack_message)),
                })
                .push(Utc)
                .push(U8Sequence::new())
                .push(tripwire.clone());

            let f = &env as (&(dyn Find<_, _> + Seq<Output = u8> + Clock));

            let (serial_pump, csq, serial_downlink) =
                standard_graph::serial(&f, writer, reader, tripwire.clone());

            let graph = standard_graph::run(
                &env,
                uplink,
                downlink_sockets,
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
