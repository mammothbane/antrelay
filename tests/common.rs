use std::{
    pin::Pin,
    sync::Arc,
};

use sluice::pipe::{
    PipeReader,
    PipeWriter,
};
use smol::{
    io::{
        AsyncRead,
        AsyncWrite,
    },
    stream::{
        Stream,
        StreamExt,
    },
};
use stream_cancel::StreamExt as _;
use tap::Pipe;

use lunarrelay::{
    message::{
        crc_wrap::RealtimeStatus,
        payload::{
            realtime_status::Flags,
            Ack,
        },
        CRCWrap,
        Message,
        OpaqueBytes,
    },
    packet_io::PacketIO,
    relay,
    stream_unwrap,
};

pub struct Harness {
    pub trigger: stream_cancel::Trigger,

    pub uplink:   async_broadcast::Sender<Message<OpaqueBytes>>,
    pub downlink: Pin<Box<dyn Stream<Item = Vec<u8>>>>,

    pub serial_read:  PipeReader,
    pub serial_write: PipeWriter,
    pub drive_serial: Pin<Box<dyn Stream<Item = Message<Ack>> + Send>>,
    pub packet_io:    Arc<PacketIO<PipeReader, PipeWriter>>,

    pub log: smol::channel::Sender<Message<OpaqueBytes>>,
}

pub fn construct_graph() -> Harness {
    let (serial_read, serial_remote_write) = sluice::pipe::pipe();
    let (serial_remote_read, serial_write) = sluice::pipe::pipe();
    let (trigger, tripwire) = stream_cancel::Tripwire::new();

    let (log_tx, log_rx) = smol::channel::unbounded();

    let (mut uplink_tx, uplink_rx) = async_broadcast::broadcast(1024);
    uplink_tx.set_overflow(true);

    let packet_io = Arc::new(PacketIO::new(serial_read, serial_write));

    let raw_serial = {
        let packet_io = packet_io.clone();
        packet_io.read_packets(0u8)
    }
    .take_until_if(tripwire.clone());

    let drive_serial = relay::relay_uplink_to_serial(
        uplink_rx.clone(),
        packet_io.clone(),
        relay::SERIAL_REQUEST_BACKOFF.clone(),
    );

    let serial_relay = relay::wrap_relay_packets(
        raw_serial,
        smol::stream::repeat(RealtimeStatus {
            memory_usage: 0,
            logs_pending: 0,
            flags:        Flags::None,
        }),
    )
    .map(|msg| msg.payload_into::<CRCWrap<OpaqueBytes>>())
    .pipe(stream_unwrap!("serializing relay packet"));

    let downlink_packets = relay::assemble_downlink(uplink_rx, log_rx, serial_relay);

    Harness {
        trigger,

        uplink: uplink_tx,
        downlink: Box::pin(downlink_packets),

        serial_read: serial_remote_read,
        serial_write: serial_remote_write,
        drive_serial: Box::pin(drive_serial),
        packet_io,

        log: log_tx,
    }
}

pub async fn mock_serial_backend(r: impl AsyncRead, w: impl AsyncWrite) {}
