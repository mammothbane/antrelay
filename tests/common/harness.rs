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
    compose,
    io::{
        pack_cobs,
        unpack_cobs,
        CommandSequencer,
    },
    message::{
        Message,
        OpaqueBytes,
    },
    standard_graph,
    util::{
        pack_message,
        unpack_message,
    },
};

use crate::common::DummyClock;

pub struct Harness {
    pub uplink:   async_broadcast::Sender<Vec<u8>>,
    pub downlink: Pin<Box<dyn Stream<Item = Vec<u8>>>>,

    pub serial_read:  PipeReader,
    pub serial_write: PipeWriter,
    pub csq:          Arc<CommandSequencer>,

    pub done:    Sender<!>,
    pub done_rx: Receiver<!>,

    pub log: Sender<Message<OpaqueBytes>>,

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

        let (drive_serial, csq, wrapped_downlink) = standard_graph::serial::<DummyClock>(
            serial_write,
            compose!(map, pack_message, |v| pack_cobs(v, 0)),
            serial_read,
            compose!(and_then, |v| unpack_cobs(v, 0), unpack_message),
            done_rx.clone(),
        );

        let (downlink_tx, downlink_rx) = smol::channel::unbounded();

        let fut = standard_graph::run(
            uplink_rx,
            unpack_message,
            std::iter::once(smol::stream::repeat(downlink_tx)),
            pack_message,
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

            log: log_tx,
        }
    }
}
