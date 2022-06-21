#![allow(unused_attributes)]
#![allow(dead_code)]
#![feature(never_type)]
#![feature(try_blocks)]
#![feature(explicit_generic_args_with_impl_trait)]

use std::{
    borrow::Borrow,
    str::FromStr,
    sync::atomic::AtomicU32,
};

use futures::{
    AsyncRead,
    AsyncWrite,
    AsyncWriteExt,
};
use smol::{
    self,
    channel::Receiver,
    stream::StreamExt,
};
use tap::Pipe;
use tracing::Instrument;
use tracing_subscriber::{
    fmt::format::FmtSpan,
    EnvFilter,
};

use antrelay::{
    futures::StreamExt as _,
    io,
    message::{
        header::{
            Destination,
            Disposition,
            RequestMeta,
            Server,
        },
        payload::Ack,
        CRCMessage,
        CRCWrap,
        Header,
        OpaqueBytes,
        StandardCRC,
    },
    trip,
    util::{
        SerialCodec,
        DEFAULT_SERIAL_CODEC,
    },
    MissionEpoch,
};

pub use harness::Harness;

mod harness;

antrelay::atomic_seq!(pub DummySeq);
antrelay::atomic_seq!(pub DummyClock, AtomicU32, u32);

pub fn trace_init() {
    let level_filter = EnvFilter::from_str("trace,async_io=warn,polling=warn").unwrap();

    tracing_subscriber::fmt()
        .with_writer(std::io::stdout)
        .with_span_events(FmtSpan::CLOSE)
        .with_env_filter(level_filter)
        .pretty()
        .init();
}

#[tracing::instrument(skip_all, err, level = "debug")]
pub async fn serial_ack_backend(
    codec: impl Borrow<SerialCodec>,
    r: impl AsyncRead + Unpin + 'static,
    w: impl AsyncWrite + Unpin,
    done: Receiver<!>,
) -> eyre::Result<()> {
    let codec = codec.borrow();

    tracing::info!("started up");

    let ser = codec.encode.clone();
    let de = codec.decode.clone();

    io::split_packets(smol::io::BufReader::new(r), DEFAULT_SERIAL_CODEC.sentinel.clone(), 1024)
        .pipe(trip!(done))
        .map(move |x| x.and_then(|x| de(x)))
        .map(|x| -> eyre::Result<CRCMessage<OpaqueBytes>> {
            try {
                let msg = x?;

                tracing::trace!(%msg, "received packet");

                let resp = CRCMessage::<_, StandardCRC>::new(
                    Header {
                        magic:       Default::default(),
                        destination: Destination::Frontend,
                        timestamp:   MissionEpoch::now(),
                        seq:         0,
                        ty:          RequestMeta {
                            disposition:         Disposition::Ack,
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

                resp.payload_into::<CRCWrap<OpaqueBytes>>()?
            }
        })
        .map(move |x| x.and_then(|x| ser(x)))
        .owned_scan(w, |mut w, resp| {
            Box::pin(async move {
                let result: eyre::Result<()> = try {
                    let resp = resp?;
                    w.write_all(&resp)
                        .instrument(tracing::trace_span!("responding to packet", ?resp))
                        .await?;
                };

                Some((w, result))
            })
        })
        .try_for_each(|x| x)
        .await?;

    Ok(())
}
