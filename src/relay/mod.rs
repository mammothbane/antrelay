use std::{
    error::Error,
    time::Duration,
};

use async_std::prelude::Stream;
use backoff::backoff::Backoff;
use smol::stream::StreamExt;
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
    },
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
