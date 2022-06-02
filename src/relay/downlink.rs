use std::error::Error;

use async_std::prelude::Stream;
use packed_struct::PackedStructSlice;
use smol::stream::StreamExt;
use tap::Pipe;

use crate::{
    message::{
        Message,
        OpaqueBytes,
    },
    net::{
        DatagramReceiver,
        DatagramSender,
    },
    stream_unwrap,
    util,
    util::splittable_stream,
};

#[tracing::instrument(skip_all)]
pub async fn send_downlink<Socket>(
    packets: impl Stream<Item = Vec<u8>> + Unpin + Send + 'static,
    streams: impl IntoIterator<Item = impl Stream<Item = Socket> + Unpin + 'static>,
) where
    Socket: DatagramSender + 'static,
    Socket::Error: Error,
{
    let (s, pump) = {
        let (split, pump) = splittable_stream(packets, 1024);

        let s = streams
            .into_iter()
            .map(|stream| {
                crate::net::send_packets(stream, split.clone())
                    .pipe(stream_unwrap!("sending downlink packet"))
                    .for_each(|_| {})
            })
            .collect::<Vec<_>>();

        (s, pump)
    };

    smol::future::zip(futures::future::join_all(s), pump).await;
}

#[tracing::instrument(skip_all)]
pub fn assemble_downlink(
    uplink: impl Stream<Item = Message<OpaqueBytes>> + Unpin + Clone + 'static,
    log_messages: impl Stream<Item = Message<OpaqueBytes>> + Unpin + 'static,
    relayed_serial_packets: impl Stream<Item = Message<OpaqueBytes>> + Unpin + 'static,
) -> impl Stream<Item = Vec<u8>> {
    uplink
        .race(log_messages)
        .race(relayed_serial_packets)
        .map(|msg| msg.pack_to_vec())
        .pipe(stream_unwrap!("packing message for downlink"))
        .map(|mut data| util::brotli_compress(&mut data))
        .pipe(stream_unwrap!("compressing message for downlink"))
}
