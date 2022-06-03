use crate::{
    io,
    message::{
        payload::{
            realtime_status::Flags,
            RealtimeStatus,
        },
        CRCWrap,
        Message,
        OpaqueBytes,
    },
    net,
    net::{
        Datagram,
        DatagramReceiver,
        DatagramSender,
    },
    relay,
    relay::wrap_relay_packets,
    stream_unwrap,
    trip,
    util::splittable_stream,
};
use packed_struct::PackedStructSlice;
use smol::{
    channel::Receiver,
    prelude::*,
};
use std::error::Error;
use tap::Pipe;

pub async fn run<Socket>(
    serial_read: impl AsyncRead + Unpin + Send + 'static,
    serial_write: impl AsyncWrite + Unpin + 'static,
    uplink: impl Stream<Item = Vec<u8>> + Unpin + Send + 'static,
    trace_event: impl Stream<Item = Message<OpaqueBytes>> + Unpin + Send + 'static,
    downlink_sockets: impl IntoIterator<Item = impl Stream<Item = Socket> + Unpin + 'static>,
    tripwire: Receiver<!>,
) where
    Socket: DatagramReceiver + DatagramSender + Send + 'static,
    Socket::Error: Error,
{
    let (uplink, uplink_pump) = uplink
        .map(|pkt| <Message<OpaqueBytes> as PackedStructSlice>::unpack_from_slice(&pkt))
        .pipe(stream_unwrap!("unpacking uplink"))
        .pipe(trip!(tripwire))
        .pipe(|s| splittable_stream(s, 1024));

    // TODO: handle cobs
    let (read_packets, pump_serial_reader) = serial_read
        .pipe(|r| io::split_packets(smol::io::BufReader::new(r), 0, 8192))
        .pipe(stream_unwrap!("splitting serial packet"))
        .pipe(|s| io::unpack_cobs_stream(s, 0))
        .pipe(stream_unwrap!("unwrapping cobs"))
        .pipe(trip!(tripwire))
        .pipe(|s| splittable_stream(s, 1024));

    let (cseq, drive_serial) =
        relay::assemble_serial(read_packets.clone(), serial_write, tripwire.clone());

    let relay_uplink =
        relay::relay_uplink_to_serial(uplink.clone(), &cseq, relay::SERIAL_REQUEST_BACKOFF.clone())
            .for_each(|_| {});

    let serial_relay = wrap_relay_packets(
        read_packets,
        smol::stream::repeat(RealtimeStatus {
            memory_usage: 0,
            logs_pending: 0,
            flags:        Flags::None,
        }),
    )
    .map(|msg| msg.payload_into::<CRCWrap<OpaqueBytes>>())
    .pipe(stream_unwrap!("serializing relay packet"));

    let downlink_packets = relay::assemble_downlink(
        uplink.clone(),
        trace_event.pipe(trip!(noclone tripwire)),
        serial_relay,
    );

    let downlink = relay::send_downlink::<Socket>(downlink_packets, downlink_sockets);

    futures::future::join5(drive_serial, pump_serial_reader, relay_uplink, uplink_pump, downlink)
        .await;
}
