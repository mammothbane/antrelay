use std::{
    error::Error,
    sync::Arc,
};

use async_std::prelude::Stream;
use backoff::backoff::Backoff;
use futures::{
    AsyncRead,
    AsyncWrite,
};
use packed_struct::PackedStructSlice;
use smol::stream::StreamExt;
use tap::Pipe;

use crate::{
    message::{
        CRCWrap,
        Message,
        OpaqueBytes,
    },
    net::{
        DatagramReceiver,
        DatagramSender,
    },
    packet_io::PacketIO,
    relay::serial,
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

pub async fn assemble_downlink<Socket, R, W>(
    uplink: impl Stream<Item = Message<OpaqueBytes>> + Unpin + Clone + 'static,
    all_serial_packets: impl Stream<Item = Message<OpaqueBytes>> + Unpin + 'static,
    log_messages: impl Stream<Item = Message<OpaqueBytes>> + Unpin + 'static,
    packet_io: Arc<PacketIO<R, W>>,
    serial_backoff: impl Backoff + Clone + 'static,
) -> impl Stream<Item = Vec<u8>>
where
    Socket: DatagramReceiver + DatagramSender,
    Socket::Error: Error + Send + Sync + 'static,
    R: AsyncRead + 'static,
    W: AsyncWrite + Unpin + 'static,
{
    let serial_acks = serial::relay_uplink_to_serial(uplink.clone(), packet_io, serial_backoff)
        .await
        .map(|msg| msg.payload_into::<CRCWrap<OpaqueBytes>>())
        .pipe(|s| log_and_discard_errors(s, "packing ack for downlink"));

    uplink
        .race(all_serial_packets)
        .race(log_messages)
        .race(serial_acks)
        .map(|msg| msg.pack_to_vec())
        .pipe(|s| log_and_discard_errors(s, "packing message for downlink"))
        .map(|mut data| util::brotli_compress(&mut data))
        .pipe(|s| log_and_discard_errors(s, "compressing message for downlink"))
}
