use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
};

use async_std::{
    channel::{
        Receiver,
        Sender,
    },
    prelude::Stream,
};
use sluice::pipe::{
    PipeReader,
    PipeWriter,
};

use antrelay::{
    io::CommandSequencer,
    message::{
        CRCMessage,
        OpaqueBytes,
    },
    standard_graph,
    util::{
        LinkCodecs,
        PacketEnv,
        SerialCodec,
        DEFAULT_LINK_CODEC,
        DEFAULT_SERIAL_CODEC,
    },
};

use crate::common::{
    DummyClock,
    DummySeq,
};

pub struct Harness {
    pub uplink:   async_broadcast::Sender<Vec<u8>>,
    pub downlink: Pin<Box<dyn Stream<Item = Vec<u8>>>>,

    pub serial_read:  PipeReader,
    pub serial_write: PipeWriter,
    pub csq:          Arc<CommandSequencer>,

    pub done:    Sender<!>,
    pub done_rx: Receiver<!>,

    pub log: Sender<CRCMessage<OpaqueBytes>>,

    pub packet_env:   Arc<PacketEnv>,
    pub serial_codec: Arc<SerialCodec>,
    pub link_codecs:  Arc<LinkCodecs>,

    pub pumps: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

impl Harness {
    pub fn new() -> Self {
        let (serial_read, serial_remote_write) = sluice::pipe::pipe();
        let (serial_remote_read, serial_write) = sluice::pipe::pipe();

        let (log_tx, log_rx) = smol::channel::unbounded();
        let (done_tx, done_rx) = smol::channel::bounded(1);

        let (mut uplink_tx, uplink_rx) = async_broadcast::broadcast(1024);
        uplink_tx.set_overflow(true);

        let packet_env = Arc::new(PacketEnv {
            clock: Arc::new(DummyClock::new()),
            seq:   Arc::new(DummySeq::new()),
        });

        let serial_codec = Arc::new(DEFAULT_SERIAL_CODEC.clone());
        let link_codecs = Arc::new(DEFAULT_LINK_CODEC.clone());

        let (drive_serial, csq, wrapped_downlink) = standard_graph::serial(
            (serial_codec.clone(), packet_env.clone()),
            serial_write,
            serial_read,
            done_rx.clone(),
        );

        let (downlink_tx, downlink_rx) = smol::channel::unbounded();

        let fut = standard_graph::run(
            (link_codecs.clone(), packet_env.clone()),
            uplink_rx,
            std::iter::once(smol::stream::repeat(downlink_tx)),
            log_rx,
            csq.clone(),
            wrapped_downlink,
            done_rx.clone(),
        );

        Harness {
            uplink: uplink_tx,
            downlink: Box::pin(downlink_rx),

            serial_read: serial_remote_read,
            serial_write: serial_remote_write,

            csq,
            done: done_tx,
            done_rx,
            pumps: Some(Box::pin(async move {
                smol::future::zip(fut, drive_serial).await;
            })),

            packet_env,
            serial_codec,
            link_codecs,

            log: log_tx,
        }
    }
}
