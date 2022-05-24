#![feature(try_blocks)]
#![feature(never_type)]

use anyhow::Result;
use lunarrelay::build;
use smol::net::unix::UnixDatagram;
use structopt::StructOpt as _;

use crate::downlink::DownlinkSockets;
use lunarrelay::util;

pub use crate::options::Options;

pub mod downlink;
pub mod trace;
pub mod uplink;

mod options;

fn main() -> Result<()> {
    eprintln!(
        "[bootstrap] starting {} {} ({}, built at {} with rustc {})",
        build::PACKAGE,
        build::VERSION,
        build::COMMIT_HASH,
        build::BUILD_TIMESTAMP,
        build::RUSTC_COMMIT_HASH
    );

    let options: Options = Options::from_args();

    eprintln!("[bootstrap] binding downlink sockets");
    let downlink_sockets = tracing::info_span!("binding downlink sockets")
        .in_scope(|| {
            (!options.disable_unix_sockets).then(|| {
                let result = DownlinkSockets::try_from(&options);
                result
            })
        })
        .transpose()?;
    eprintln!("[bootstrap] downlink sockets bound");

    trace::init(downlink_sockets.as_ref())?;
    tracing::info!(
        application = build::PACKAGE,
        version = build::VERSION,
        build_commit = build::COMMIT_HASH,
        built_at = build::BUILD_TIMESTAMP,
        using_rustc = build::RUSTC_COMMIT_HASH,
        "tracing subsystem initialized"
    );

    let uplink_socket = tracing::info_span!("binding uplink sockets")
        .in_scope(|| {
            (!options.disable_unix_sockets).then(|| util::uds_connect(options.uplink_sock))
        })
        .transpose()?;

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

    let (uplink_task, downlink_task) = tracing::info_span!("starting link comms").in_scope(|| {
        let uplink_task = uplink_socket.map(|uplink_socket| {
            let fut = uplink::uplink(uplink_socket, serial_write, signal_done.clone());

            smol::spawn(fut)
        });

        let downlink_task = downlink_sockets.map(|downlink_sockets| {
            let fut = downlink::downlink(downlink_sockets, serial_read, signal_done.clone());

            smol::spawn(fut)
        });

        (uplink_task, downlink_task)
    });

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
