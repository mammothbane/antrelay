#![feature(try_blocks)]
#![feature(never_type)]
#![feature(proc_macro_hygiene)]
#![feature(stmt_expr_attributes)]
#![feature(let_else)]
#![feature(iter_intersperse)]
#![deny(unsafe_code)]

use actix::Supervisor;
use eyre::Result;
use net::{
    DatagramOps,
    DatagramReceiver,
    DatagramSender,
};
use runtime::{
    ground,
    ground::downlink::StaticSender,
    serial,
};
use std::sync::Arc;
use structopt::StructOpt as _;
use util::build;

pub use crate::options::Options;

pub mod trace;

mod options;

#[cfg(windows)]
type Socket = tokio::net::UdpSocket;

#[cfg(unix)]
type Socket = tokio::net::unix::UnixDatagram;

#[actix::main]
async fn main() -> Result<()> {
    util::bootstrap!(
        "starting {} {} ({}, built at {} with rustc {})",
        build::PACKAGE,
        build::VERSION,
        build::COMMIT_HASH,
        build::BUILD_TIMESTAMP,
        build::RUSTC_COMMIT_HASH,
    );

    let options: Options = Options::from_args();

    trace::init();

    tracing::info!(
        application = %build::PACKAGE,
        version = %build::VERSION,
        build_commit = %build::COMMIT_HASH,
        built_at = %build::BUILD_TIMESTAMP,
        using_rustc = %build::RUSTC_COMMIT_HASH,
        "tracing subsystem initialized"
    );

    Supervisor::start(|_ctx| runtime::StateMachine::default());
    Supervisor::start(|_ctx| serial::Serial);

    options.downlink_addresses.into_iter()
        .for_each(|addr| {
            Supervisor::start(move |_ctx| {
                ground::downlink::Downlink::new(Box::new(move || {
                    Box::pin(async move {
                        match <Socket as DatagramOps>::bind(&addr).await {
                            Ok(sock) => Some(Arc::new(sock) as Arc<StaticSender<<Socket as DatagramSender>::Error>>),
                            Err(e) => {
                                tracing::error!(?addr, error = %e, "unable to connect to downlink socket");
                                None
                            },
                        }
                    })
                }))
            });
        });

    Supervisor::start(move |_ctx| ground::uplink::Uplink {
        make_socket: Box::new(move || {
            Box::pin(async move {
                match <Socket as DatagramOps>::connect(&options.uplink_address).await {
                    Ok(sock) => {
                        let b: Box<
                            dyn DatagramReceiver<Error = <Socket as DatagramReceiver>::Error>
                                + Unpin
                                + 'static,
                        > = Box::new(sock);
                        Some(b)
                    },
                    Err(e) => {
                        tracing::error!(error = %e, "connecting to uplink socket");
                        None
                    },
                }
            })
        }),
    });

    Supervisor::start(|_ctx| serial::raw::RawIO::new(Box::new(|| Box::pin(async move { None }))));

    Ok(())
}
