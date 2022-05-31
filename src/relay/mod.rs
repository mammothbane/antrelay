use std::{
    error::Error,
    time::Duration,
};

use async_std::prelude::Stream;
use backoff::backoff::Backoff;
use futures::AsyncWrite;
use smol::{
    io::AsyncRead,
    stream::StreamExt,
};
use tap::Pipe;

use crate::{
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
    net::{
        receive_packets,
        DatagramReceiver,
        DatagramSender,
    },
    packet_io::PacketIO,
    util::{
        deserialize_messages,
        log_and_discard_errors,
        splittable_stream,
    },
    MissionEpoch,
};

mod downlink;
mod serial;

pub use downlink::*;
pub use serial::*;

lazy_static::lazy_static! {
    pub static ref SERIAL_REQUEST_BACKOFF: backoff::exponential::ExponentialBackoff<backoff::SystemClock> = backoff::ExponentialBackoffBuilder::new() .with_initial_interval(Duration::from_millis(25))
        .with_max_interval(Duration::from_secs(1))
        .with_randomization_factor(0.5)
        .with_max_elapsed_time(Some(Duration::from_secs(3)))
        .build();
}

pub async fn graph<Socket>(
    done: smol::channel::Receiver<!>,
    serial_read: impl AsyncRead + Unpin + 'static,
    serial_write: impl AsyncWrite + Unpin + 'static,
    serial_request_backoff: impl Backoff + Clone + 'static,
    uplink: impl Stream<Item = Message<OpaqueBytes>> + Send + 'static,
    log_stream: impl Stream<Item = Message<OpaqueBytes>> + Unpin + 'static,
) -> impl Stream<Item = Vec<u8>> + Unpin
where
    Socket: DatagramReceiver + DatagramSender,
    Socket::Error: Error + Send + Sync + 'static,
{
    let packet_io = PacketIO::new(smol::io::BufReader::new(serial_read), serial_write, done);
    let packet_io = Box::leak(Box::new(packet_io));

    let all_serial = packet_io.read_packets(0u8).await;

    let uplink_split = splittable_stream(uplink, 1024);

    assemble_downlink::<Socket, _, _>(
        uplink_split,
        all_serial,
        log_stream,
        packet_io,
        serial_request_backoff,
    )
    .await
}

pub async fn uplink_stream<Socket>(
    address: Socket::Address,
    backoff: impl Backoff + Clone + Send + Sync + 'static,
    buffer: usize,
) -> impl Stream<Item = Message<OpaqueBytes>>
where
    Socket: DatagramReceiver + Send + Sync + 'static,
    Socket::Address: Send + Clone + Sync,
    Socket::Error: Error + Send + Sync + 'static,
{
    receive_packets::<Socket>(address, backoff)
        .pipe(deserialize_messages)
        .pipe(|s| log_and_discard_errors(s, "deserializing messages"))
        .pipe(|s| splittable_stream(s, buffer))
}

// TODO: use actual log format
pub fn dummy_log_downlink(
    s: impl Stream<Item = crate::tracing::Event>,
) -> impl Stream<Item = Message<OpaqueBytes>> {
    s.pipe(|s| futures::stream::StreamExt::chunks(s, 5)).map(|evts| {
        let payload = evts
            .into_iter()
            .map(|evt| {
                evt.args
                    .into_iter()
                    .map(|(name, value)| format!("{}={}", name, value))
                    .intersperse(",".to_owned())
                    .collect::<String>()
            })
            .intersperse(":".to_owned())
            .collect::<String>();

        let payload = payload.as_bytes().iter().map(|x| *x).collect::<Vec<u8>>();

        let wrapped_payload = CRCWrap::<Vec<u8>>::new(payload);

        Message {
            header:  Header {
                magic:       Default::default(),
                destination: Destination::Ground,
                timestamp:   MissionEpoch::now(),
                seq:         0,
                ty:          Type {
                    ack:                   true,
                    acked_message_invalid: false,
                    target:                Target::Frontend,
                    kind:                  Kind::Ping,
                },
            },
            payload: wrapped_payload,
        }
    })
}
