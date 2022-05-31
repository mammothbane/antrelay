#![feature(try_blocks)]
#![feature(never_type)]
#![feature(proc_macro_hygiene)]
#![feature(stmt_expr_attributes)]
#![feature(let_else)]
#![feature(explicit_generic_args_with_impl_trait)]
#![feature(iter_intersperse)]

extern crate core;

use eyre::Result;
use packed_struct::PackedStructSlice;
use smol::stream::StreamExt;
use structopt::StructOpt as _;
use tap::Pipe;

use lunarrelay::{
    build,
    message::{
        header::{
            Destination,
            Kind,
            Target,
            Type,
        },
        CRCWrap,
        Header,
        Message,
    },
    net,
    net::{
        receive_packets,
        DEFAULT_BACKOFF,
    },
    packet_io::PacketIO,
    signals,
    util::{
        self,
        log_and_discard_errors,
        splittable_stream,
    },
    MissionEpoch,
};

pub use crate::options::Options;
use crate::relay::deserialize_messages;

pub mod trace;

mod options;
mod relay;

#[cfg(windows)]
type Socket = smol::net::UdpSocket;

#[cfg(unix)]
type Socket = smol::net::unix::UnixDatagram;

fn main() -> Result<()> {
    util::bootstrap!(
        "starting {} {} ({}, built at {} with rustc {})",
        build::PACKAGE,
        build::VERSION,
        build::COMMIT_HASH,
        build::BUILD_TIMESTAMP,
        build::RUSTC_COMMIT_HASH,
    );

    let options: Options = Options::from_args();

    #[cfg(unix)]
    smol::block_on(lunarrelay::util::dynload::apply_patches(&options.lib_dir));

    let log_stream = trace::init()?;

    tracing::info!(
        downlink_ty = ?lunarrelay::message::payload::log::Type::Startup,
        application = build::PACKAGE,
        version = build::VERSION,
        build_commit = build::COMMIT_HASH,
        built_at = build::BUILD_TIMESTAMP,
        using_rustc = build::RUSTC_COMMIT_HASH,
        "tracing subsystem initialized"
    );

    let serial = tracing::info_span!("opening serial port").in_scope(
        || -> Result<async_compat::Compat<tokio_serial::SerialStream>> {
            let builder = tokio_serial::new(options.serial_port, options.baud);

            let stream = smol::block_on(async move {
                async_compat::Compat::new(async { tokio_serial::SerialStream::open(&builder) })
                    .await
            })?;

            Ok(async_compat::Compat::new(stream))
        },
    )?;
    let (serial_read, serial_write) = smol::io::split(serial);

    let signal_done = signals::signals()?;

    let packet_rpc =
        PacketIO::new(smol::io::BufReader::new(serial_read), serial_write, signal_done.clone());

    smol::block_on({
        let packet_rpc = Box::leak(Box::new(packet_rpc));

        async move {
            let all_serial_packets = packet_rpc.read_packets(0u8).await;

            let uplink_stream =
                receive_packets::<Socket>(options.uplink_address.clone(), DEFAULT_BACKOFF.clone())
                    .pipe(deserialize_messages)
                    .pipe(|s| log_and_discard_errors(s, "deserializing messages"))
                    .pipe(|s| splittable_stream(s, 1024));

            let serial_relay =
                relay::serial_relay(uplink_stream.clone(), &packet_rpc).await.for_each(|_| {});

            // TODO: dumb test format
            let log_messages =
                log_stream.pipe(|s| futures::stream::StreamExt::chunks(s, 5)).map(|evts| {
                    let payload = evts
                        .into_iter()
                        .map(|evt| {
                            evt.args
                                .into_iter()
                                .map(|(name, value)| format!("{}={}", name, value))
                                .intersperse(",".to_owned())
                                .collect::<String>()
                        })
                        .intersperse(":".to_owned())
                        .collect::<String>();

                    let payload = payload.as_bytes().iter().map(|x| *x).collect::<Vec<u8>>();

                    let wrapped_payload = CRCWrap::<Vec<u8>>::new(payload);

                    Message {
                        header:  Header {
                            magic:       Default::default(),
                            destination: Destination::Ground,
                            timestamp:   MissionEpoch::now(),
                            seq:         0,
                            ty:          Type {
                                ack:                   true,
                                acked_message_invalid: false,
                                target:                Target::Frontend,
                                kind:                  Kind::Ping,
                            },
                        },
                        payload: wrapped_payload,
                    }
                });

            let downlink_collected = uplink_stream
                .race(all_serial_packets)
                .race(log_messages)
                .map(|msg| msg.pack_to_vec())
                .pipe(|s| log_and_discard_errors(s, "packing message for downlink"))
                .map(|mut data| compress(&mut data))
                .pipe(|s| log_and_discard_errors(s, "compressing message for downlink"));

            let downlink_split = downlink_collected.pipe(|s| splittable_stream(s, 1024));

            let downlink = options
                .downlink_addresses
                .into_iter()
                .map(move |addr| {
                    net::send_packets::<Socket>(
                        addr,
                        downlink_split.clone(),
                        lunarrelay::net::DEFAULT_BACKOFF.clone(),
                    )
                })
                .pipe(futures::future::join_all);

            smol::future::zip(downlink, serial_relay).await;
        }
    });

    Ok(())
}

pub fn compress(message: &mut Vec<u8>) -> Result<Vec<u8>> {
    let compressed_bytes = {
        let mut out = vec![];

        lazy_static::lazy_static! {
            static ref PARAMS: brotli::enc::BrotliEncoderParams = brotli::enc::BrotliEncoderParams {
                quality: 11,
                ..Default::default()
            };
        }

        brotli::BrotliCompress(&mut &message[..], &mut out, &*PARAMS)?;

        out
    };

    Ok(compressed_bytes)
}
