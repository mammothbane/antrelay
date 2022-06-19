use std::{
    borrow::Borrow,
    time::Duration,
};

use async_std::prelude::Stream;
use smol::stream::StreamExt;
use tap::Pipe;

use crate::{
    message::{
        header::{
            Conversation,
            Destination,
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
    util::PacketEnv,
    MissionEpoch,
};

mod control;
mod downlink;
mod serial;

pub use downlink::*;
pub use serial::*;

lazy_static::lazy_static! {
    pub static ref SERIAL_REQUEST_BACKOFF: backoff::exponential::ExponentialBackoff<backoff::SystemClock> = backoff::ExponentialBackoffBuilder::new()
        .with_initial_interval(Duration::from_millis(25))
        .with_max_interval(Duration::from_secs(1))
        .with_randomization_factor(0.5)
        .with_max_elapsed_time(Some(Duration::from_secs(3)))
        .build();
}

#[tracing::instrument(skip_all)]
pub fn ack_uplink_packets<'e, 's, 'o>(
    env: impl Borrow<PacketEnv> + 'e,
    packets: impl Stream<Item = Message<OpaqueBytes>> + 's,
) -> impl Stream<Item = Message<Ack>> + 'o
where
    'e: 'o,
    's: 'o,
{
    packets
        .filter(|pkt| pkt.header.destination == Destination::Frontend)
        .map(move |pkt| {
            Ok(Message {
                header: Header::downlink(env.borrow(), pkt.header.ty.conversation_type),

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
#[tracing::instrument(skip_all)]
pub fn dummy_log_downlink<'e, 's, 'o>(
    env: impl Borrow<PacketEnv> + 'e,
    s: impl Stream<Item = crate::tracing::Event> + 's,
) -> impl Stream<Item = Message<OpaqueBytes>> + 'o
where
    'e: 'o,
    's: 'o,
{
    s.pipe(|s| futures::stream::StreamExt::chunks(s, 2))
        .map(|evts| bincode::serialize(&evts))
        .pipe(stream_unwrap!("serializing log payload"))
        .map(move |payload| -> Message<OpaqueBytes> {
            let wrapped_payload = CRCWrap::<Vec<u8>>::new(payload);

            Message {
                header:  Header::downlink(env.borrow(), Conversation::Ping),
                payload: wrapped_payload,
            }
        })
}
