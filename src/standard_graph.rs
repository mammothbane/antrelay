use std::{
    error::Error,
    pin::Pin,
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
    split,
    stream_monad::{
        self,
        StreamCodec,
    },
    stream_unwrap,
    trip,
    util::{
        brotli_compress,
        splittable_stream,
    },
};

type StdCodec =

pub fn run<Socket, ReadCodec, WriteCodec>(
    serial_downlink: impl AsyncRead + Unpin + Send + 'static,
    serial_uplink: impl AsyncWrite + Unpin + 'static,
    uplink: impl Stream<Item = Message<OpaqueBytes>> + Unpin + Send + 'static,
    trace_event: impl Stream<Item = Message<OpaqueBytes>> + Unpin + Send + 'static,
    downlink_sockets: impl IntoIterator<Item = impl Stream<Item = Socket> + Unpin + 'static>,
    tripwire: Receiver<!>,
) -> (impl Future<Output = ()>, Arc<CommandSequencer>)
where
    Socket: DatagramReceiver + DatagramSender + Send + 'static,
    Socket::Error: Error,
    ReadCodec: StreamCodec<Input = Vec<u8>, Output = Vec<u8>>,
    WriteCodec: StreamCodec<Input = Vec<u8>, Output = Vec<u8>>,
    ReadCodec::Error: Error + 'static,
{
    let (uplink, uplink_pump) = uplink.pipe(trip!(tripwire)).pipe(|s| splittable_stream(s, 1024));

    // TODO: handle cobs
    let (read_packets, pump_serial_reader) = serial_downlink
        .pipe(|r| io::split_packets(smol::io::BufReader::new(r), 0, 8192))
        .pipe(stream_unwrap!("splitting serial downlink packets"))
        .pipe(ReadCodec::encode)
        .pipe(stream_unwrap!("parsing serial downlink packets"))
        .pipe(trip!(tripwire))
        .pipe(split!());

    let (csq, drive_serial) =
        relay::assemble_serial(read_packets.clone(), serial_uplink, tripwire.clone());

    let csq = Arc::new(csq);

    let relay_uplink = relay::relay_uplink_to_serial(
        uplink.clone(),
        csq.clone(),
        relay::SERIAL_REQUEST_BACKOFF.clone(),
    )
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

    let fut_joined = async move {
        futures::future::join5(
            drive_serial,
            pump_serial_reader,
            relay_uplink,
            uplink_pump,
            downlink,
        )
        .await;
    };

    (fut_joined, csq)
}
