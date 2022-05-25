#![feature(never_type)]
#![feature(io_safety)]

use std::{
    net::Shutdown,
    path::Path,
};

use lunarrelay::util;
use structopt::StructOpt;

mod options;
mod trace;

pub use options::Options;

fn main() -> eyre::Result<()> {
    trace::init();

    let options: Options = Options::from_args();
    let done = util::signals()?;

    smol::block_on(async move {
        let _uplink = util::remove_and_bind(&options.uplink_sock).await?;

        let tasks = [
            options.telemetry_sock.clone(),
            options.reliable_sock.clone(),
            options.store_and_forward_sock.clone(),
        ]
        .map(|path| smol::spawn(log_all(path, done.clone())));

        let _ = done.recv().await;

        tracing::info!("main task received interrupt, awaiting log tasks");

        futures::future::join_all(tasks).await;

        Ok(()) as eyre::Result<()>
    })
}

#[tracing::instrument(fields(path = ?socket_path.as_ref()), skip(socket_path, done))]
async fn log_all(
    socket_path: impl AsRef<Path>,
    done: smol::channel::Receiver<!>,
) -> eyre::Result<()> {
    let mut buf = vec![0; 4096];

    let socket = util::remove_and_bind(&socket_path).await?;

    loop {
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
        tracing::info!(length = slice.len(), "message received");
    }

    socket.shutdown(Shutdown::Both)?;
    smol::fs::remove_file(socket_path).await?;

    Ok(())
}
