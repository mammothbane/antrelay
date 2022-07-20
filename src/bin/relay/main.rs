#![feature(try_blocks)]
#![feature(never_type)]
#![feature(proc_macro_hygiene)]
#![feature(stmt_expr_attributes)]
#![feature(let_else)]
#![feature(iter_intersperse)]
#![deny(unsafe_code)]

use actix::{
    Supervisor,
    System,
};
use std::sync::Arc;
use structopt::StructOpt as _;

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
use util::build;

pub use crate::options::Options;

mod options;
pub mod trace;

#[cfg(windows)]
type Socket = tokio::net::UdpSocket;

#[cfg(unix)]
type Socket = tokio::net::unix::UnixDatagram;

fn main() -> std::io::Result<()> {
    util::bootstrap!(
        "starting {} {} ({}, built at {} with rustc {})",
        build::PACKAGE,
        build::VERSION,
        build::COMMIT_HASH,
        build::BUILD_TIMESTAMP,
        build::RUSTC_COMMIT_HASH,
    );

    let options: Options = Options::from_args();

    trace::init(options.pretty);

    tracing::info!(
        application = %build::PACKAGE,
        version = %build::VERSION,
        build_commit = %build::COMMIT_HASH,
        built_at = %build::BUILD_TIMESTAMP,
        using_rustc = %build::RUSTC_COMMIT_HASH,
        "tracing subsystem initialized"
    );

    let sys = System::new();

    sys.block_on(async {
        Supervisor::start(|_ctx| runtime::StateMachine::default());
        Supervisor::start(|_ctx| serial::Serial);

        options.downlink_addresses.into_iter()
            .for_each(|addr| {
                Supervisor::start(move |_ctx| {
                    ground::downlink::Downlink::new(Box::new(move || {
                        Box::pin(async move {
                            match <Socket as DatagramOps>::connect(&addr).await {
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
                    match <Socket as DatagramOps>::bind(&options.uplink_address).await {
                        Ok(sock) => {
                            let b: Box<
                                dyn DatagramReceiver<Error = <Socket as DatagramReceiver>::Error>
                                    + Unpin
                                    + Send
                                    + Sync
                                    + 'static,
                            > = Box::new(sock);
                            Some(b)
                        },
                        Err(e) => {
                            tracing::error!(error = %e, "binding uplink socket");
                            None
                        },
                    }
                })
            }),
        });

        Supervisor::start(move |_ctx| {
            serial::raw::RawIO::new(Box::new(move || {
                let port = options.serial_port.clone();

                Box::pin(async move {
                    let ser = tokio_serial::new(&port, options.baud);

                    match tokio_serial::SerialStream::open(&ser) {
                        Ok(s) => {
                            let (r, w) = tokio::io::split(s);
                            Some(((Box::new(r) as Box<dyn tokio::io::AsyncRead + Unpin + 'static>), Box::new(w) as Box<dyn tokio::io::AsyncWrite + Unpin + 'static>))
                        },
                        Err(e) => {
                            tracing::error!(error = %e, "connecting to serial port");
                            None
                        }
                    }
                })
            }))
        });

        tracing::info!("all systems started");
    });

    sys.run()
}
