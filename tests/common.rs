#![allow(unused_attributes)]
#![feature(never_type)]
#![feature(try_blocks)]

use futures::{
    AsyncRead,
    AsyncWrite,
    AsyncWriteExt,
};
use packed_struct::PackedStructSlice;
use std::{
    future::Future,
    pin::Pin,
    str::FromStr,
    sync::Arc,
};

use sluice::pipe::{
    PipeReader,
    PipeWriter,
};
use smol::{
    self,
    channel::{
        Receiver,
        Sender,
    },
    stream::{
        Stream,
        StreamExt,
    },
};
use tap::Pipe;
use tracing::Instrument;
use tracing_subscriber::{
    fmt::format::FmtSpan,
    EnvFilter,
};

use lunarrelay::{
    futures::StreamExt as _,
    io,
    io::{
        pack_cobs,
        CommandSequencer,
    },
    message::{
        header::{
            Destination,
            Disposition,
            RequestMeta,
            Server,
        },
        payload::{
            realtime_status::Flags,
            Ack,
            RealtimeStatus,
        },
        CRCWrap,
        Header,
        Message,
        OpaqueBytes,
        StandardCRC,
    },
    relay,
    split,
    standard_graph,
    stream_unwrap,
    trip,
    MissionEpoch,
};

pub struct Harness {
    pub uplink:   async_broadcast::Sender<Message<OpaqueBytes>>,
    pub downlink: Pin<Box<dyn Stream<Item = Vec<u8>>>>,

    pub serial_read:  PipeReader,
    pub serial_write: PipeWriter,
    pub csq:          Arc<CommandSequencer>,

    pub done:    Sender<!>,
    pub done_rx: Receiver<!>,

    pub log: Sender<Message<OpaqueBytes>>,

    pub pumps: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

pub fn construct_graph() -> Harness {
    let (serial_read, serial_remote_write) = sluice::pipe::pipe();
    let (serial_remote_read, serial_write) = sluice::pipe::pipe();

    let (log_tx, log_rx) = smol::channel::unbounded();
    let (done_tx, done_rx) = smol::channel::bounded(1);

    let (mut uplink_tx, uplink_rx) = async_broadcast::broadcast(1024);
    uplink_tx.set_overflow(true);

    let (fut, csq) = standard_graph::run(
        serial_read,
        serial_write,
        uplink_rx.clone(),
        log_rx,
        std::iter::empty(),
        done_rx.clone(),
    );

    Harness {
        uplink: uplink_tx,
        downlink: Box::pin(downlink_packets),

        serial_read: serial_remote_read,
        serial_write: serial_remote_write,

        csq,
        done: done_tx,
        done_rx,
        pumps: Some(Box::pin(fut)),

        log: log_tx,
    }
}

pub fn trace_init() {
    let level_filter = EnvFilter::from_str("debug").unwrap();

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_span_events(FmtSpan::CLOSE)
        .with_env_filter(level_filter)
        .pretty()
        .init();
}

#[tracing::instrument(skip_all, err, level = "debug")]
pub async fn serial_ack_backend(
    r: impl AsyncRead + Unpin + 'static,
    w: impl AsyncWrite + Unpin,
    done: Receiver<!>,
) -> eyre::Result<()> {
    tracing::info!("started up");

    io::split_packets(smol::io::BufReader::new(r), 0, 1024)
        .pipe(trip!(done))
        .then(|pkt| {
            tracing::debug!("received packet");
            Box::pin(async move {
                let pkt = io::unpack_cobs(pkt?, 0)?;

                let msg = <Message<OpaqueBytes> as PackedStructSlice>::unpack_from_slice(&pkt)?;
                let resp = Message::<_, StandardCRC>::new(
                    Header {
                        magic:       Default::default(),
                        destination: Destination::Frontend,
                        timestamp:   MissionEpoch::now(),
                        seq:         0,
                        ty:          RequestMeta {
                            disposition:         Disposition::Response,
                            request_was_invalid: false,
                            server:              Server::CentralStation,
                            conversation_type:   msg.header.ty.conversation_type,
                        },
                    },
                    Ack {
                        timestamp: msg.header.timestamp,
                        seq:       msg.header.seq,
                        checksum:  msg.payload.checksum()?[0],
                    },
                );

                let result = resp.pack_to_vec()?;
                let result = pack_cobs(result, 0);

                Ok(result) as eyre::Result<Vec<u8>>
            })
        })
        .owned_scan(w, |mut w, resp| {
            Box::pin(async move {
                let result: eyre::Result<()> = try {
                    let resp = resp?;
                    w.write_all(&resp)
                        .instrument(tracing::debug_span!("responding to packet", ?resp))
                        .await?;
                };

                Some((w, result))
            })
        })
        .try_for_each(|x| x)
        .await?;

    Ok(())
}
