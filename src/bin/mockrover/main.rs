#![feature(never_type)]
#![feature(io_safety)]
#![feature(explicit_generic_args_with_impl_trait)]
#![cfg(feature = "serial_cobs")]

use std::{
    sync::{
        atomic::{
            AtomicU8,
            Ordering,
        },
        Arc,
    },
    time::Duration,
};

use async_compat::Compat;
use packed_struct::PackedStructSlice;
use smol::{
    future::Future,
    io::AsyncWriteExt,
    lock::Mutex,
    stream::{
        Stream,
        StreamExt as _,
    },
};
use stream_cancel::StreamExt as _;
use structopt::StructOpt;
use tap::Pipe;

use lunarrelay::{
    message::{
        header::{
            Destination,
            Kind,
            Source,
            Type,
        },
        CRCWrap,
        Header,
        Message,
        OpaqueBytes,
    },
    net::{
        receive_packets,
        socket_stream,
        Datagram,
        SocketMode,
        DEFAULT_BACKOFF,
    },
    signals,
    stream_unwrap,
    tracing::Event,
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

    let done = signals::signals()?;
    tracing::info!("registered signals");

    let (tripwire, trigger) = stream_cancel::Tripwire::new();

    smol::spawn(async move {
        done.recv().await.unwrap_err();
        tracing::info!("interrupted");
        tripwire.cancel();
    })
    .detach();

    smol::block_on(async move {
        let _uplink_sock = <Socket as Datagram>::bind(&uplink_sock).await?;
        tracing::info!(addr = ?uplink_sock, "bound uplink socket");

        let downlink_fut = downlink
            .into_iter()
            .map(|dl| log_all(dl, trigger.clone()))
            .pipe(futures::future::join_all);

        let serial_stream = send_serial(serial_port).await?;
        let serial_fut = serial_stream
            .pipe(stream_unwrap!("handling serial"))
            .take_until_if(trigger)
            .for_each(|_| ());

        smol::future::zip(downlink_fut, serial_fut).await;

        Ok(()) as eyre::Result<()>
    })
}

async fn send_serial(path: String) -> eyre::Result<impl Stream<Item = eyre::Result<()>>> {
    let builder = tokio_serial::new(&path, 115200);

    let serial_stream = {
        let compat_stream =
            Compat::new(async { tokio_serial::SerialStream::open(&builder) }).await?;
        Arc::new(Mutex::new(Compat::new(compat_stream)))
    };

    let seq = Arc::new(AtomicU8::new(0));

    Ok(smol::Timer::interval(Duration::from_secs(5)).then(move |_inst| {
        let serial_stream = serial_stream.clone();

        let seq = seq.clone();

        Box::pin({
            async move {
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
                            source:                Source::Rover,
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
                tracing::trace!("wrote to serial port");

                Ok(()) as eyre::Result<()>
            }
        })
    }))
}

#[tracing::instrument(fields(addr = ?addr), skip(done), err(Display))]
async fn log_all(
    addr: <Socket as Datagram>::Address,
    done: impl Future<Output = bool>,
) -> eyre::Result<()> {
    let downlink_sockets =
        socket_stream::<Socket>(addr.clone(), DEFAULT_BACKOFF.clone(), SocketMode::Bind)
            .pipe(stream_unwrap!("connecting to downlink socket"));

    receive_packets(downlink_sockets)
        .take_until_if(done)
        .map(|packet| {
            tracing::trace!(length = packet.len(), "packet received");

            let mut decompressed = vec![];
            brotli::BrotliDecompress(&mut &packet[..], &mut decompressed)?;
            Ok(decompressed) as eyre::Result<Vec<u8>>
        })
        .pipe(stream_unwrap!("decompressing message"))
        .map(|decompressed| {
            <Message<OpaqueBytes> as PackedStructSlice>::unpack_from_slice(&decompressed)
        })
        .pipe(stream_unwrap!("unpacking message"))
        .map(|msg: Message<OpaqueBytes>| {
            if msg.header.ty.kind == Kind::Ping && msg.header.ty.source == Source::Frontend {
                let payload = bincode::deserialize::<Vec<Event>>(&msg.payload.as_ref())?;
                tracing::debug!(msg.header = %msg.header.display(), ?payload);
            } else {
                tracing::debug!(msg.header = %msg.header.display(), payload.len = msg.payload.as_ref().len());
            }

            Ok(()) as eyre::Result<()>
        })
        .pipe(stream_unwrap!("deserializing message payload"))
        .for_each(|_| {})
        .await;

    #[cfg(unix)]
    smol::fs::remove_file(addr).await?;

    Ok(())
}
