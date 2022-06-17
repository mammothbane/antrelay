use std::{
    error::Error,
    sync::Arc,
};

use smol::{
    channel::Receiver,
    prelude::*,
};
use tap::Pipe;

use crate::{
    io,
    io::CommandSequencer,
    message::{
        header::Conversation,
        payload::{
            realtime_status::Flags,
            RealtimeStatus,
        },
        CRCWrap,
        Header,
        Message,
        OpaqueBytes,
    },
    net::DatagramSender,
    relay,
    relay::wrap_relay_packets,
    split,
    stream_unwrap,
    trip,
    util::{
        splittable_stream,
        GroundLinkCodec,
        PacketEnv,
        SerialCodec,
    },
};

pub fn serial<'s, 'p, 'o>(
    (codec, packet_env): (&'s SerialCodec, &'p PacketEnv),
    serial_uplink_io: impl AsyncWrite + Unpin + 'static,
    serial_downlink_io: impl AsyncRead + Unpin + Send + 'static,
    tripwire: Receiver<!>,
) -> (
    impl Future<Output = ()> + 'o,
    Arc<CommandSequencer>,
    impl Stream<Item = Message<OpaqueBytes>> + 'o,
)
where
    's: 'o,
    'p: 'o,
{
    let (raw_serial_downlink, pump_serial_reader) = serial_downlink_io
        .pipe(|r| io::split_packets(smol::io::BufReader::new(r), 0, 8192))
        .pipe(stream_unwrap!("splitting serial downlink packets"))
        .pipe(trip!(tripwire))
        .pipe(split!());

    let serial_downlink_packets = raw_serial_downlink
        .clone()
        .map(&codec.deserialize)
        .pipe(stream_unwrap!("parsing serial downlink packets"));

    let (csq, serial_ack_results, serial_uplink_packets) =
        CommandSequencer::new(serial_downlink_packets);

    let encoded_serial_packets = serial_uplink_packets
        .map(&codec.serialize)
        .pipe(trip!(tripwire))
        .pipe(stream_unwrap!("encoding serial uplink packets"));

    let pump_serial_uplink = io::write_packet_stream(encoded_serial_packets, serial_uplink_io)
        .pipe(stream_unwrap!("uplinking serial packet"))
        .for_each(|_| {});

    let pump_acks = serial_ack_results.pipe(stream_unwrap!("reading serial ack")).for_each(|_| {});
    let drive_serial = async move {
        futures::future::join3(pump_acks, pump_serial_reader, pump_serial_uplink).await;
    };

    let csq = Arc::new(csq);

    let wrapped_serial_downlink = wrap_relay_packets(
        packet_env,
        raw_serial_downlink,
        smol::stream::repeat(RealtimeStatus {
            memory_usage: 0,
            logs_pending: 0,
            flags:        Flags::None,
        }),
    )
    .map(|msg| msg.payload_into::<CRCWrap<OpaqueBytes>>())
    .pipe(stream_unwrap!("converting"));

    (drive_serial, csq, wrapped_serial_downlink)
}

pub async fn run<Socket>(
    (link_codec, packet_env): (&GroundLinkCodec, &PacketEnv),
    uplink: impl Stream<Item = Vec<u8>> + Unpin + Send,
    downlink_sockets: impl IntoIterator<Item = impl Stream<Item = Socket> + Unpin>,
    trace_event: impl Stream<Item = Message<OpaqueBytes>> + Unpin + Send,
    csq: Arc<CommandSequencer>,
    wrapped_serial_downlink: impl Stream<Item = Message<OpaqueBytes>> + Unpin + Send,
    tripwire: Receiver<!>,
) where
    Socket: DatagramSender + Send + 'static,
    Socket::Error: Error,
{
    let (raw_uplink, uplink_pump) =
        uplink.pipe(trip!(tripwire)).pipe(|s| splittable_stream(s, 1024));

    let uplink = raw_uplink
        .clone()
        .map(&link_codec.deserialize)
        .pipe(stream_unwrap!("unpacking uplink packet"));

    let pump_uplink_to_serial =
        relay::relay_uplink_to_serial(uplink, csq.clone(), relay::SERIAL_REQUEST_BACKOFF.clone())
            .for_each(|_| {});

    let wrapped_uplink = raw_uplink.map(|uplink_pkt| {
        Message::new(Header::downlink(packet_env, Conversation::Relay), uplink_pkt)
    });

    let downlink_packets = futures::stream_select![
        wrapped_uplink,
        trace_event.pipe(trip!(noclone tripwire)),
        wrapped_serial_downlink
    ]
    .map(&link_codec.serialize)
    .pipe(stream_unwrap!("encoding downlink message"));

    let downlink = relay::send_downlink::<Socket>(downlink_packets, downlink_sockets);

    futures::future::join3(pump_uplink_to_serial, uplink_pump, downlink).await;
}
