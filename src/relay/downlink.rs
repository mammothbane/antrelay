use std::error::Error;

use async_std::prelude::Stream;
use packed_struct::PackedStructSlice;
use smol::stream::StreamExt;
use tap::Pipe;
use tracing::Instrument;

use crate::{
    message::{
        Message,
        OpaqueBytes,
    },
    net::DatagramSender,
    stream_unwrap,
    util,
    util::splittable_stream,
};

#[tracing::instrument(skip_all, level = "debug")]
pub async fn send_downlink<Socket>(
    packets: impl Stream<Item = Vec<u8>> + Unpin + Send + 'static,
    streams: impl IntoIterator<Item = impl Stream<Item = Socket> + Unpin + 'static>,
) where
    Socket: DatagramSender + 'static,
    Socket::Error: Error,
{
    let (split, pump) = splittable_stream(packets, 1024);

    let s = {
        let split = split;

        streams
            .into_iter()
            .map(|stream| {
                crate::net::send_packets(stream, split.clone())
                    .pipe(stream_unwrap!("sending downlink packet"))
                    .for_each(|_| {})
                    .instrument(tracing::debug_span!("downlink single socket packet relaying"))
            })
            .collect::<Vec<_>>()
    };

    smol::future::zip(
        pump.instrument(tracing::debug_span!("downlink pump")),
        futures::future::join_all(s).instrument(tracing::debug_span!("downlink packet relaying")),
    )
    .await;
}

#[tracing::instrument(skip_all)]
pub fn assemble_downlink(
    uplink: impl Stream<Item = Message<OpaqueBytes>> + Unpin,
    log_messages: impl Stream<Item = Message<OpaqueBytes>> + Unpin,
    relayed_serial_packets: impl Stream<Item = Message<OpaqueBytes>> + Unpin,
) -> impl Stream<Item = Vec<u8>> {
    futures::stream_select![uplink, log_messages, relayed_serial_packets]
        .map(|msg| msg.pack_to_vec())
        .pipe(stream_unwrap!("packing message for downlink"))
        .map(|mut data| util::brotli_compress(&mut data))
        .pipe(stream_unwrap!("compressing message for downlink"))
}
