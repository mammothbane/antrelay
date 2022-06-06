use std::error::Error;

use async_std::prelude::Stream;
use smol::stream::StreamExt;
use tap::Pipe;

use crate::{
    net::DatagramSender,
    stream_unwrap,
    util::splittable_stream,
};

#[tracing::instrument(skip_all, level = "trace")]
pub async fn send_downlink<Socket>(
    packets: impl Stream<Item = Vec<u8>> + Unpin + Send,
    streams: impl IntoIterator<Item = impl Stream<Item = Socket> + Unpin>,
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
            })
            .collect::<Vec<_>>()
    };

    smol::future::zip(pump, futures::future::join_all(s)).await;
}
