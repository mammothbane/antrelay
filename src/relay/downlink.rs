use std::error::Error;

use async_std::prelude::Stream;
use backoff::backoff::Backoff;
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
    util,
    util::{
        log_and_discard_errors,
        splittable_stream,
    },
};

pub async fn send_downlink<Socket>(
    packets: impl Stream<Item = Vec<u8>> + Unpin + Send + 'static,
    addresses: impl IntoIterator<Item = Socket::Address>,
    backoff: impl Backoff + Clone,
) where
    Socket: DatagramSender,
    Socket::Error: Error,
{
    {
        let split = splittable_stream(packets, 1024);

        addresses.into_iter().map(move |addr| {
            crate::net::send_packets::<Socket>(addr, split.clone(), backoff.clone())
        })
    }
    .pipe(futures::future::join_all)
    .await;
}

pub async fn assemble_downlink<Socket>(
    uplink: impl Stream<Item = Message<OpaqueBytes>> + Unpin + Clone + 'static,
    all_serial_packets: impl Stream<Item = Message<OpaqueBytes>> + Unpin + 'static,
    log_messages: impl Stream<Item = Message<OpaqueBytes>> + Unpin + 'static,
) -> impl Stream<Item = Vec<u8>>
where
    Socket: DatagramReceiver + DatagramSender,
    Socket::Error: Error + Send + Sync + 'static,
{
    uplink
        .race(all_serial_packets)
        .race(log_messages)
        .map(|msg| msg.pack_to_vec())
        .pipe(|s| log_and_discard_errors(s, "packing message for downlink"))
        .map(|mut data| util::brotli_compress(&mut data))
        .pipe(|s| log_and_discard_errors(s, "compressing message for downlink"))
}
