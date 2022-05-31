#![feature(never_type)]
#![feature(io_safety)]
#![cfg(feature = "serial_cobs")]

use std::{
    net::Shutdown,
    sync::atomic::{
        AtomicU8,
        Ordering,
    },
    time::Duration,
};

use async_compat::Compat;
use packed_struct::PackedStructSlice;
use smol::{
    io::AsyncWriteExt,
    lock::Mutex,
    stream::StreamExt,
};
use structopt::StructOpt;
use tap::Pipe;

use lunarrelay::{
    message::{
        header::{
            Destination,
            Kind,
            Target,
            Type,
        },
        CRCWrap,
        Header,
        Message,
        OpaqueBytes,
    },
    trace_catch,
    util,
    util::net::Datagram,
    MissionEpoch,
};

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
        serial_port,
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

        smol::spawn(async move {
            loop {
                trace_catch!(send_serial(serial_port), "sending dummy serial data");
                smol::Timer::after(Duration::from_secs(1));
            }
        })
        .detach();

        let _ = done.recv().await;

        tracing::info!("main task received interrupt, awaiting log tasks");

        Ok(()) as eyre::Result<()>
    })
}

async fn send_serial(path: String) -> eyre::Result<()> {
    let builder = tokio_serial::new(&path, 115200);

    let serial_stream = {
        let compat_stream =
            Compat::new(async { tokio_serial::SerialStream::open(&builder) }).await?;
        Mutex::new(Compat::new(compat_stream))
    };

    let seq = AtomicU8::new(0);

    smol::Timer::interval(Duration::from_secs(5))
        .then(|_inst| {
            let serial_stream = &serial_stream;

            Box::pin(async move {
                let mut serial_stream = serial_stream.lock().await;

                let wrapped_payload = CRCWrap::<Vec<u8>>::new(b"test".to_vec());

                let message = Message {
                    header:  Header {
                        magic:       Default::default(),
                        destination: Destination::CentralStation,
                        timestamp:   MissionEpoch::now(),
                        seq:         seq.fetch_add(1, Ordering::Acquire),
                        ty:          Type {
                            ack:                   false,
                            acked_message_invalid: false,
                            target:                Target::CentralStation,
                            kind:                  Kind::VoltageSupplied,
                        },
                    },
                    payload: wrapped_payload,
                };

                let message = message.pack_to_vec()?;

                let encoded = {
                    let mut ret = cobs::encode_vec(&message);
                    ret.push(0);
                    ret
                };

                serial_stream.write_all(&encoded).await?;
                tracing::debug!("wrote to serial port");

                Ok(()) as eyre::Result<()>
            })
        })
        .try_for_each(|result| result)
        .await
}

#[tracing::instrument(fields(addr = ?addr), skip(done))]
async fn log_all(
    addr: <Socket as Datagram>::Address,
    done: smol::channel::Receiver<!>,
) -> eyre::Result<()> {
    let mut buf = vec![0; 4096];

    let socket = <Socket as Datagram>::bind(&addr).await?;
    tracing::info!("bound downlink socket");

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

        tracing::debug!(length = count, "message received");

        let mut decompressed = vec![];
        brotli::BrotliDecompress(&mut &buf[..count], &mut decompressed)?;

        let msg = <Message<OpaqueBytes> as PackedStructSlice>::unpack_from_slice(&decompressed)?;
        tracing::debug!(%msg.header, %msg.payload, "decoded message");
    }

    socket.shutdown(Shutdown::Both)?;

    #[cfg(unix)]
    smol::fs::remove_file(addr).await?;

    Ok(())
}
