#![allow(unused_attributes)]
#![allow(dead_code)]
#![feature(never_type)]
#![feature(try_blocks)]
#![feature(explicit_generic_args_with_impl_trait)]

use std::{
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
    compose,
    futures::StreamExt as _,
    io,
    io::{
        pack_cobs,
        unpack_cobs,
    },
    message::{
        header::{
            Destination,
            Disposition,
            RequestMeta,
            Server,
        },
        payload::Ack,
        Header,
        Message,
        OpaqueBytes,
        StandardCRC,
    },
    trip,
    util::{
        pack_message,
        unpack_message,
    },
    MissionEpoch,
};

pub use harness::Harness;

mod harness;

antrelay::atomic_seq!(pub DummyClock, AtomicU32, u32);

pub fn trace_init() {
    let level_filter = EnvFilter::from_str("debug").unwrap();

    tracing_subscriber::fmt()
        .with_writer(std::io::stdout)
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
        .map(compose!(
            and_then,
            std::convert::identity,
            |v| unpack_cobs(v, 0),
            unpack_message::<OpaqueBytes>,
            |msg: Message<OpaqueBytes>| {
                tracing::debug!("received packet");

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

                Ok(resp)
            },
            pack_message,
            |v| Ok(pack_cobs(v, 0))
        ))
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
