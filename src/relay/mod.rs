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
            Destination,
            Event,
            RequestMeta,
        },
        payload::Ack,
        CRCMessage,
        CRCWrap,
        Header,
        OpaqueBytes,
    },
    stream_unwrap,
    util::PacketEnv,
};

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
    packets: impl Stream<Item = CRCMessage<OpaqueBytes>> + 's,
) -> impl Stream<Item = CRCMessage<Ack>> + 'o
where
    'e: 'o,
    's: 'o,
{
    packets
        .filter(|pkt| pkt.header.destination == Destination::Frontend)
        .map(move |pkt| {
            Ok(CRCMessage {
                header: Header::downlink(env.borrow(), pkt.header.ty.event),

                payload: CRCWrap::new(Ack {
                    timestamp: pkt.header.timestamp,
                    seq:       pkt.header.seq,
                    checksum:  pkt.payload.checksum()?[0],
                }),
            }) as eyre::Result<CRCMessage<Ack>>
        })
        .pipe(stream_unwrap!("checksumming message"))
}

// TODO: use actual log format
#[tracing::instrument(skip_all)]
pub fn dummy_log_downlink<'e, 's, 'o>(
    env: impl Borrow<PacketEnv> + 'e,
    s: impl Stream<Item = crate::tracing::Event> + 's,
) -> impl Stream<Item = CRCMessage<OpaqueBytes>> + 'o
where
    'e: 'o,
    's: 'o,
{
    s.pipe(|s| futures::stream::StreamExt::chunks(s, 2))
        .map(|evts| bincode::serialize(&evts))
        .pipe(stream_unwrap!("serializing log payload"))
        .map(move |payload| -> CRCMessage<OpaqueBytes> {
            let wrapped_payload = CRCWrap::<Vec<u8>>::new(payload);

            CRCMessage {
                header:  Header {
                    ty: RequestMeta::PONG,
                    ..Header::downlink(env.borrow(), Event::FE_PING)
                },
                payload: wrapped_payload,
            }
        })
}
