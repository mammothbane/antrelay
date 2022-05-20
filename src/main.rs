#![feature(never_type)]

use std::thread;

use anyhow::Result;
use async_std::{
    io,
    sync,
};
use futures::AsyncWriteExt;
use log::{
    Metadata,
    Record,
};
use smol::net::unix::UnixDatagram;

mod log;
mod options;
mod message;

fn main() -> Result<()> {
    let options: options::Options = options::Options::from_args()?;

    let telemetry = UnixDatagram::bind(options.telemetry_sock)?;
    let reliable = UnixDatagram::bind(options.reliable_sock)?;
    let store_and_forward = UnixDatagram::bind(options.store_and_forward_sock)?;

    let done = signals()?;

    tracing::

    Ok(())
}

fn signals() -> Result<smol::channel::Receiver<!>> {
    let (tx, rx) = smol::channel::unbounded();

    thread::spawn(move || {
        use signal_hook::{
            consts::*,
            iterator::Signals,
        };

        let mut signals = Signals::new(&[SIGTERM, SIGINT]).expect("registering signal handlers");

        signals.wait().next();

        tx.close();
    });

    Ok(rx)
}
