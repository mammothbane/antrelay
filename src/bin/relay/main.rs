#![feature(try_blocks)]
#![feature(never_type)]
#![feature(proc_macro_hygiene)]
#![feature(stmt_expr_attributes)]
#![feature(let_else)]

use eyre::Result;
use lunarrelay::build;
use structopt::StructOpt as _;

use lunarrelay::util::{
    self,
    net::{
        receive_packets,
        Datagram,
        DatagramReceiver,
        DatagramSender,
        DEFAULT_BACKOFF,
    },
    send_packets,
};

pub use crate::options::Options;
use crate::packet_io::PacketIO;

// pub mod downlink;
pub mod trace;
// pub mod uplink;

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

    let (downlink_tx, downlink_rx) = async_broadcast::broadcast(1024);

    let downlink_task = util::tee_packets(
        options.downlink_sockets.clone(),
        *util::net::DEFAULT_BACKOFF,
        downlink_rx,
    );

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

    let signal_done = util::signals()?;

    let (serial_read, serial_write) = smol::io::split(serial);
    let packet_rpc =
        PacketIO::new(smol::io::BufReader::new(serial_read), serial_write, signal_done.clone());

    let uplink_stream = receive_packets(options.uplink_socket.clone(), *DEFAULT_BACKOFF);
    packet_rpc.read_messages()

    relay::relay(uplink_stream, downlink_tx, packet_rpc);

    smol::block_on(async move {
        let _ = signal_done.recv().await;
        tracing::info!("main task interrupted");

        let _span = tracing::info_span!("waiting for links to shutdown").entered();

        match (uplink_task, downlink_task) {
            (Some(task), None) => {
                task.await;
            },

            (None, Some(task)) => {
                task.await;
            },

            (Some(task1), Some(task2)) => {
                smol::future::zip(task1, task2).await;
            },

            _ => {},
        }
    });

    Ok(())
}
