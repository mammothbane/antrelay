use std::{
    borrow::Borrow,
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
        CRCMessage,
        CRCWrap,
        Header,
        OpaqueBytes,
    },
    net::DatagramSender,
    relay,
    relay::{
        control::State,
        wrap_serial_packets_for_downlink,
    },
    split,
    stream_unwrap,
    trip,
    util::{
        splittable_stream,
        LinkCodecs,
        PacketEnv,
        SerialCodec,
    },
};

pub fn serial<'s, 'p, 'o, 'os>(
    (codec, packet_env): (impl Borrow<SerialCodec> + 's, impl Borrow<PacketEnv> + 'p),
    serial_uplink_io: impl AsyncWrite + Unpin + 'static,
    serial_downlink_io: impl AsyncRead + Unpin + Send + 'static,
    tripwire: Receiver<!>,
) -> (
    impl Future<Output = ()> + 'o,
    Arc<CommandSequencer>,
    impl Stream<Item = CRCMessage<OpaqueBytes>> + 'os,
)
where
    'p: 'os,
{
    let codec = codec.borrow();

    let (raw_serial_downlink, pump_serial_reader) = serial_downlink_io
        .pipe(|r| io::split_packets(smol::io::BufReader::new(r), 0, 8192))
        .pipe(stream_unwrap!("splitting serial downlink packets"))
        .pipe(trip!(tripwire))
        .pipe(split!());

    let ser = codec.encode.clone();
    let de = codec.decode.clone();

    let serial_downlink_packets = raw_serial_downlink
        .clone()
        .map(move |x| de(x))
        .pipe(stream_unwrap!("parsing serial downlink packets"));

    let (csq, serial_ack_results, serial_uplink_packets) =
        CommandSequencer::new(serial_downlink_packets);

    let (encoded_serial_packets, serial_command_pump) = serial_uplink_packets
        .map(move |x| ser(x))
        .pipe(trip!(tripwire))
        .pipe(stream_unwrap!("encoding serial uplink packets"))
        .pipe(split!());

    let pump_serial_uplink =
        io::write_packet_stream(encoded_serial_packets.clone(), serial_uplink_io)
            .pipe(stream_unwrap!("uplinking serial packet"))
            .for_each(|_| {});

    let pump_acks = serial_ack_results.pipe(stream_unwrap!("reading serial ack")).for_each(|_| {});
    let drive_serial = async move {
        futures::future::join4(
            pump_acks,
            pump_serial_reader,
            pump_serial_uplink,
            serial_command_pump,
        )
        .await;
    };

    let csq = Arc::new(csq);

    let wrapped_serial_downlink = wrap_serial_packets_for_downlink(
        packet_env,
        raw_serial_downlink,
        smol::stream::repeat(RealtimeStatus {
            memory_usage: 0,
            logs_pending: 0,
            flags:        Flags::None,
        }),
    )
    .map(|msg| {
        let result = msg.payload_into::<CRCWrap<OpaqueBytes>>();

        tracing::trace!(from = %msg, to = ?result, "convert serial message");

        result
    })
    .pipe(stream_unwrap!("converting"));

    (drive_serial, csq, wrapped_serial_downlink)
}

pub async fn run<Socket>(
    (link_codec, packet_env): (impl Borrow<LinkCodecs>, impl Borrow<PacketEnv> + Send),
    uplink: impl Stream<Item = Vec<u8>> + Unpin + Send,
    downlink_sockets: impl IntoIterator<Item = impl Stream<Item = Socket> + Unpin>,
    trace_event: impl Stream<Item = CRCMessage<OpaqueBytes>> + Unpin + Send,
    csq: Arc<CommandSequencer>,
    wrapped_serial_downlink: impl Stream<Item = CRCMessage<OpaqueBytes>> + Unpin + Send,
    tripwire: Receiver<!>,
) where
    Socket: DatagramSender + Send + 'static,
    Socket::Error: Error,
{
    let link_codec = link_codec.borrow();
    let packet_env = packet_env.borrow();

    let (raw_uplink, uplink_pump) =
        uplink.pipe(trip!(tripwire)).pipe(|s| splittable_stream(s, 1024));

    let ser = link_codec.downlink.encode.clone();
    let de = link_codec.uplink.decode.clone();

    let uplink = raw_uplink.clone().map(&*de).pipe(stream_unwrap!("unpacking uplink packet"));

    let pump_uplink_to_serial = relay::relay_uplink_to_serial(
        packet_env,
        State::FlightIdle,
        uplink,
        csq.clone(),
        relay::SERIAL_REQUEST_BACKOFF.clone(),
    );

    let wrapped_uplink = raw_uplink.map(move |uplink_pkt| {
        CRCMessage::new(Header::downlink(packet_env, Conversation::Relay), uplink_pkt)
    });

    let downlink_packets =
        futures::stream_select![wrapped_uplink, trace_event, wrapped_serial_downlink]
            .map(&*ser)
            .pipe(trip!(tripwire))
            .pipe(stream_unwrap!("encoding downlink message"));

    let downlink = relay::send_downlink::<Socket>(downlink_packets, downlink_sockets);

    futures::future::join3(pump_uplink_to_serial, uplink_pump, downlink).await;
}

#[cfg(test)]
mod test {
    use smol::channel::Sender;

    use crate::util::{
        timeout,
        DEFAULT_LINK_CODEC,
        DEFAULT_SERIAL_CODEC,
    };

    use super::*;

    #[async_std::test]
    async fn serial_shutdown() {
        let (tx_done, rx_done) = smol::channel::bounded(1);

        let (pump, _cs, s) = serial(
            (DEFAULT_SERIAL_CODEC.clone(), PacketEnv::default()),
            smol::io::sink(),
            smol::io::repeat(0u8),
            rx_done,
        );

        let task = smol::spawn(async move {
            smol::future::zip(s.for_each(|_| {}), pump).await;
        });

        tx_done.close();

        timeout(500, task).await.unwrap();
    }

    #[async_std::test]
    async fn run_shutdown() {
        let (tx_done, rx_done) = smol::channel::bounded(1);

        let (csq, s1, s2) = CommandSequencer::new(smol::stream::empty());
        let (tx, rx) = smol::channel::unbounded();

        smol::spawn(async move {
            let f1 = s1.for_each(|_| {});
            let f2 = s2.for_each(|_| {});
            let f3 = rx.for_each(|_| {});

            futures::future::join3(f1, f2, f3).await;
        })
        .detach();

        let task = smol::spawn(run::<Sender<Vec<u8>>>(
            (DEFAULT_LINK_CODEC.clone(), PacketEnv::default()),
            smol::stream::repeat(vec![]),
            std::iter::once(smol::stream::repeat(tx)),
            smol::stream::empty(),
            Arc::new(csq),
            smol::stream::repeat_with(|| {
                CRCMessage::new(
                    Header::cs_command(&PacketEnv::default(), Conversation::Calibrate),
                    vec![],
                )
            }),
            rx_done,
        ));

        tx_done.close();

        timeout(500, task).await.unwrap();
    }
}
