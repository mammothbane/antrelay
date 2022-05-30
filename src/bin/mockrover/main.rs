#![feature(never_type)]
#![feature(io_safety)]

use std::net::Shutdown;

use lunarrelay::{
    util,
    util::net::Datagram,
};
use structopt::StructOpt;
use tap::Pipe;

mod options;
mod trace;

pub use options::Options;

#[cfg(windows)]
type Socket = smol::net::UdpSocket;

#[cfg(unix)]
type Socket = smol::net::unix::UnixDatagram;

fn main() -> eyre::Result<()> {
    trace::init();

    let Options {
        downlink,
        uplink_sock,
        ..
    } = Options::from_args();

    let done = util::signals()?;

    tracing::info!("registered signals");

    smol::block_on(async move {
        let _uplink_sock = <Socket as Datagram>::bind(&uplink_sock).await?;
        tracing::info!(addr = ?uplink_sock, "bound uplink socket");

        let downlink_fut = downlink
            .into_iter()
            .map(|dl| log_all(dl, done.clone()))
            .pipe(futures::future::join_all);

        tracing::info!("logging messages from downlink");
        smol::spawn(downlink_fut).detach();

        let _ = done.recv().await;

        tracing::info!("main task received interrupt, awaiting log tasks");

        Ok(()) as eyre::Result<()>
    })
}

#[tracing::instrument(fields(path = ?socket_path), skip(socket_path, done))]
async fn log_all(
    socket_path: <Socket as Datagram>::Address,
    done: smol::channel::Receiver<!>,
) -> eyre::Result<()> {
    let mut buf = vec![0; 4096];

    let socket = <Socket as Datagram>::bind(&socket_path).await?;
    tracing::debug!("bound downlink socket");

    loop {
        tracing::debug!("waiting for packet");

        let count = match util::either(socket.recv(&mut buf), done.recv()).await {
            either::Left(Ok(count)) => count,
            either::Left(Err(e)) => {
                tracing::error!(e = %e, "failed to read from socket");
                continue;
            },
            either::Right(Err(smol::channel::RecvError)) => {
                tracing::info!("logger shutting down");
                break;
            },
            either::Right(Ok(_)) => unreachable!(),
        };

        let slice = &buf[..count];
        tracing::debug!(length = slice.len(), "message received");
    }

    socket.shutdown(Shutdown::Both)?;

    #[cfg(unix)]
    smol::fs::remove_file(socket_path).await?;

    Ok(())
}
