use async_std::prelude::FutureExt;
use std::{
    any::Any,
    cell::RefCell,
    collections::HashMap,
    sync::{
        atomic,
        atomic::Ordering,
    },
    time::Duration,
};

use async_std::sync::{
    Arc,
    Mutex,
};
use futures::{
    AsyncWriteExt,
    FutureExt,
};
use packed_struct::PackedStructSlice;
use smol::{
    io::{
        AsyncBufRead,
        AsyncBufReadExt,
        AsyncRead,
        AsyncWrite,
    },
    stream::{
        Stream,
        StreamExt,
    },
};
use tracing::warn;
use tracing_subscriber::filter::FilterExt;

use crate::packet_io::PacketIO;
use lunarrelay::{
    message::{
        payload,
        payload::Ack,
        Message,
        OpaqueBytes,
    },
    util,
    util::either,
};

#[derive(
    Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
pub struct MessageId(lunarrelay::MissionEpoch, u8);

#[inline]
pub fn serialize_messages(
    s: impl Stream<Item = Message<OpaqueBytes>>,
) -> impl Stream<Item = eyre::Result<Vec<u8>>> {
    s.map(|msg| msg.pack_to_vec())
}

pub async fn relay<R, W>(
    uplink: impl Stream<Item = Message<OpaqueBytes>>,
    packetio: PacketIO<R, W>,
) -> impl Stream<Item = Message<OpaqueBytes>> {
    uplink.then(|msg| {
        let packetio = &packetio;

        let mut futs = vec![
            async {
                // todo: received packet relay format
                // todo: local message storage?
                let relay = downlink.send(msg.clone()).await;
                lunarrelay::trace_catch!(relay, "failed relaying message back to downlink");
            }
            .boxed(),
        ];

        if msg.header.destination != lunarrelay::message::header::Destination::Frontend {
            let fut = async {
                let req = retry_serial(packetio, msg).await;
                lunarrelay::trace_catch!(req, "no ack from serial connection");
            };

            futs.push(fut.boxed());
        }

        futures::future::join_all(futs.into_iter())
    })
}

#[tracing::instrument(fields(msg = ?msg), skip(cb, responses))]
async fn retry_serial<R, W>(
    pio: &PacketIO<R, W>,
    msg: Message<OpaqueBytes>,
) -> Result<Message<Ack>, backoff::Error<eyre::Report>>
where
    W: AsyncWrite + Unpin,
{
    let backoff = backoff::ExponentialBackoffBuilder::new()
        .with_initial_interval(Duration::from_millis(10))
        .with_max_interval(Duration::from_secs(2))
        .with_randomization_factor(0.5)
        .with_max_elapsed_time(Some(Duration::from_secs(3)))
        .build();

    backoff::future::retry_notify(
        backoff,
        || async {
            let g = pio.request(msg).await?;
            let ret = g.wait().await?;

            Ok(ret)
        },
        |e, dur| {
            tracing::error!(error = %e, "retrieving from serial");
        },
    )
    .await
}
