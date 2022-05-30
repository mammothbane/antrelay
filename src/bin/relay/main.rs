#![feature(try_blocks)]
#![feature(never_type)]
#![feature(proc_macro_hygiene)]
#![feature(stmt_expr_attributes)]
#![feature(let_else)]
#![feature(explicit_generic_args_with_impl_trait)]

extern crate core;

use eyre::Result;
use lunarrelay::{
    build,
    trace_catch,
};
use packed_struct::PackedStructSlice;
use smol::stream::StreamExt;
use structopt::StructOpt as _;
use tap::Pipe;

use lunarrelay::util::{
    self,
    log_and_discard_errors,
    net::{
        receive_packets,
        DEFAULT_BACKOFF,
    },
    send_packets,
    splittable_stream,
};

pub use crate::options::Options;
use crate::{
    packet_io::PacketIO,
    relay::deserialize_messages,
};

pub mod trace;

mod options;
mod packet_io;
mod relay;

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

    #[cfg(unix)]
    smol::block_on(lunarrelay::util::dynload::apply_patches(&options.lib_dir));

    let log_stream = trace::init()?;

    tracing::info!(
        downlink_ty = ?lunarrelay::message::crc_wrap::log::Type::Startup,
        application = build::PACKAGE,
        version = build::VERSION,
        build_commit = build::COMMIT_HASH,
        built_at = build::BUILD_TIMESTAMP,
        using_rustc = build::RUSTC_COMMIT_HASH,
        "tracing subsystem initialized"
    );

    let serial = tracing::info_span!("opening serial port").in_scope(
        || -> Result<async_compat::Compat<tokio_serial::SerialStream>> {
            let builder = tokio_serial::new(options.serial_port, options.baud);

            let stream = smol::block_on(async move {
                async_compat::Compat::new(async { tokio_serial::SerialStream::open(&builder) })
                    .await
            })?;

            Ok(async_compat::Compat::new(stream))
        },
    )?;
    let (serial_read, serial_write) = smol::io::split(serial);

    let signal_done = util::signals()?;

    let packet_rpc =
        PacketIO::new(smol::io::BufReader::new(serial_read), serial_write, signal_done.clone());

    smol::block_on({
        let packet_rpc = Box::leak(Box::new(packet_rpc));

        async move {
            let all_serial_packets = packet_rpc.read_packets(0u8).await;

            let uplink_stream =
                receive_packets::<Socket>(options.uplink_socket.clone(), DEFAULT_BACKOFF.clone())
                    .pipe(deserialize_messages)
                    .pipe(|s| log_and_discard_errors(s, "deserializing messages"))
                    .pipe(|s| splittable_stream(s, 1024));

            relay::serial_relay(uplink_stream.clone(), &packet_rpc).await.for_each(|_| {}).await;

            let downlink_collected = uplink_stream.race(all_serial_packets)
                .map(|msg| msg.pack_to_vec()) // TODO: compress
                .pipe(|s| log_and_discard_errors(s, "packing message for downlink"));

            let downlink_split = downlink_collected.pipe(|s| splittable_stream(s, 1024));

            let downlink_fut = options
                .downlink_sockets
                .into_iter()
                .map(move |addr| {
                    send_packets::<Socket>(
                        addr,
                        downlink_split.clone(),
                        util::net::DEFAULT_BACKOFF.clone(),
                    )
                })
                .pipe(futures::future::join_all);

            let _ = signal_done.recv().await;
            tracing::info!("main task interrupted");

            let _span = tracing::info_span!("waiting for links to shutdown").entered();
        }
    });

    Ok(())
}
