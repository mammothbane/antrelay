#![feature(never_type)]
#![feature(try_blocks)]

use std::net::Shutdown;
use std::time::Duration;

use anyhow::Result;
use smol::io::{
    AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt,
};
use smol::net::unix::UnixDatagram;
use structopt::StructOpt as _;

use crate::downlink::DownlinkSockets;
use crate::options::Options;

mod downlink;
mod message;
mod options;
mod trace;
mod util;

fn main() -> Result<()> {
    eprintln!("[bootstrap] starting {} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    let options: Options = Options::from_args();

    let span = tracing::info_span!("binding downlink sockets").entered();
    eprintln!("[bootstrap] binding downlink sockets");
    let downlink_sockets = (!options.disable_unix_sockets)
        .then(|| {
            let result = downlink::DownlinkSockets::try_from(&options);
            result
        })
        .transpose()?;
    eprintln!("[bootstrap] downlink sockets bound");
    drop(span);

    trace::init(downlink_sockets.as_ref())?;
    tracing::info!("log subsystem initialized");

    let span = tracing::info_span!("binding uplink sockets").entered();
    let uplink_socket = (!options.disable_unix_sockets)
        .then(|| util::uds_connect(options.uplink_sock))
        .transpose()?;
    drop(span);

    let span = tracing::info_span!("opening serial port").entered();
    let serial = {
        let builder = tokio_serial::new(options.serial_port, options.baud);

        let stream = smol::block_on(async move {
            async_compat::Compat::new(async { tokio_serial::SerialStream::open(&builder) }).await
        })?;

        async_compat::Compat::new(stream)
    };
    let (serial_read, serial_write) = smol::io::split(serial);
    drop(span);

    let signal_done = util::signals()?;

    let span = tracing::info_span!("starting link comms").entered();
    let uplink_task = uplink_socket.map(|uplink_socket| smol::spawn(uplink(uplink_socket, serial_write, signal_done.clone())));
    let downlink_task = downlink_sockets.map(|socks| smol::spawn(downlink(socks, serial_read, signal_done.clone())));
    drop(span);

    smol::block_on(async move {
        let _ = signal_done.recv().await;

        tracing::info!("main task interrupted");

        let span = tracing::info_span!("waiting for links to shutdown").entered();

        match (uplink_task, downlink_task) {
            (Some(x), None)  => {
                x.await;
            },
            (None, Some(x)) => {
                x.await;
            },
            (Some(x), Some(y)) => {
                smol::future::zip(x, y).await;
            },
            _ => {},
        }
    });

    Ok(())
}

async fn uplink(uplink_socket: UnixDatagram, mut serial_write: impl AsyncWrite + Unpin, done: smol::channel::Receiver<!>) {
    let mut buf = vec![0; 4096];

    loop {
        let result: Result<_> = try {
            let count = match util::either(uplink_socket.recv(&mut buf), done.recv()).await {
                either::Left(ret) => ret?,
                either::Right(_) => {
                    if let Err(e) = uplink_socket.shutdown(Shutdown::Both) {
                        tracing::error!(error = ?e, "shutting down uplink socket after done signal");
                    }

                    return;
                },
            };

            serial_write.write_all(&buf[..count]).await?;
        };

        if let Err(e) = result {
            tracing::error!(error = ?e, "uplink error, sleeping before retry");
            smol::Timer::after(Duration::from_millis(20)).await;
        }
    }
}

async fn downlink(downlink_sockets: DownlinkSockets, serial_read: impl AsyncRead + Unpin, done: smol::channel::Receiver<!>) {
    let mut buf = vec![0; 4096];

    let mut serial_read = smol::io::BufReader::new(serial_read);

    loop {
        let result: Result<_> = try {
            // todo: framing pending fz spec
            let count = match util::either(serial_read.read_until(b'\n', &mut buf), done.recv()).await {
                either::Left(ret) => ret?,
                either::Right(_) => {
                    if let Err(e) = downlink_sockets.shutdown(Shutdown::Both) { // shutdown
                        tracing::error!(error = ?e, "shutting down downlink sockets after done signal");
                    }

                    return;
                },
            };

            let data: &[u8] = &buf[..count];
            let framed_message = message::Downlink::Data(data);

            let framed_bytes = rmp_serde::to_vec(&framed_message)?;
            downlink_sockets.send_all(&framed_bytes).await?;

            // TODO: what do we actually want the strategy to be here re: where to send what?
        };

        if let Err(e) = result {
            tracing::error!(error = ?e, "downlink error, sleeping before retry");
            smol::Timer::after(Duration::from_millis(20)).await;
        }
    }
}
