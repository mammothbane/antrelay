use std::time::Duration;

use async_std::prelude::Stream;
use smol::stream::StreamExt;
use tap::Pipe;
use tracing::Span;

use crate::{
    message::{
        header::{
            Conversation,
            Destination,
            Disposition,
            RequestMeta,
            Server,
        },
        payload::Ack,
        CRCWrap,
        Header,
        Message,
        OpaqueBytes,
    },
    stream_unwrap,
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

#[tracing::instrument(skip_all)]
pub fn ack_frontend(
    packets: impl Stream<Item = Message<OpaqueBytes>>,
) -> impl Stream<Item = Message<Ack>> {
    packets
        .filter(|pkt| pkt.header.destination == Destination::Frontend)
        .map(|pkt| {
            Ok(Message {
                header:  Header {
                    magic:       Default::default(),
                    destination: Destination::Ground,
                    timestamp:   MissionEpoch::now(),
                    seq:         0,
                    ty:          RequestMeta {
                        disposition:         Disposition::Response,
                        request_was_invalid: false,
                        server:              Server::Frontend,
                        conversation_type:   pkt.header.ty.conversation_type,
                    },
                },
                payload: CRCWrap::new(Ack {
                    timestamp: pkt.header.timestamp,
                    seq:       pkt.header.seq,
                    checksum:  pkt.payload.checksum()?[0],
                }),
            }) as eyre::Result<Message<Ack>>
        })
        .pipe(stream_unwrap!("checksumming message"))
}

// TODO: use actual log format
#[tracing::instrument(skip(s))]
pub fn dummy_log_downlink(
    s: impl Stream<Item = crate::tracing::Event>,
) -> impl Stream<Item = Message<OpaqueBytes>> {
    let span = Span::current();

    s.pipe(|s| futures::stream::StreamExt::chunks(s, 2))
        .map(|evts| -> eyre::Result<Message<OpaqueBytes>> {
            let payload = bincode::serialize(&evts)?;
            let wrapped_payload = CRCWrap::<Vec<u8>>::new(payload);

            Ok(Message {
                header:  Header {
                    magic:       Default::default(),
                    destination: Destination::Ground,
                    timestamp:   MissionEpoch::now(),
                    seq:         0,
                    ty:          RequestMeta {
                        disposition:         Disposition::Response,
                        request_was_invalid: false,
                        server:              Server::Frontend,
                        conversation_type:   Conversation::Ping,
                    },
                },
                payload: wrapped_payload,
            })
        })
        .pipe(stream_unwrap!(parent: &span, "serializing log payload"))
}
