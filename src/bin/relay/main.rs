#![feature(try_blocks)]
#![feature(never_type)]
#![feature(proc_macro_hygiene)]
#![feature(stmt_expr_attributes)]
#![feature(let_else)]
#![feature(explicit_generic_args_with_impl_trait)]
#![feature(iter_intersperse)]

use eyre::Result;
use smol::stream::StreamExt as _;
use structopt::StructOpt as _;

use lunarrelay::{
    build,
    io,
    message::{
        payload::{
            realtime_status::Flags,
            RealtimeStatus,
        },
        CRCWrap,
        OpaqueBytes,
    },
    net,
    net::{
        SocketMode,
        DEFAULT_BACKOFF,
    },
    relay,
    relay::wrap_relay_packets,
    signals,
    stream_unwrap,
    util,
    util::splittable_stream,
};
use tap::Pipe;
use tracing::Instrument;

pub use crate::options::Options;

pub mod trace;

mod options;

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

    let trace_event_stream = trace::init()?;

    tracing::info!(
        downlink_ty = ?lunarrelay::message::payload::log::Type::Startup,
        application = build::PACKAGE,
        version = build::VERSION,
        build_commit = build::COMMIT_HASH,
        built_at = build::BUILD_TIMESTAMP,
        using_rustc = build::RUSTC_COMMIT_HASH,
        "tracing subsystem initialized"
    );

    let signal_done = signals::signals()?;

    smol::block_on({
        async move {
            #[cfg(unix)]
            util::dynload::apply_patches(&options.lib_dir).await;

            let (reader, writer) = relay::connect_serial(options.serial_port, options.baud).await?;

            let (trigger, tripwire) = smol::channel::bounded::<!>(1);

            smol::spawn(async move {
                signal_done.recv().await.unwrap_err();
                trigger.close();
            })
            .detach();

            let uplink_sockets = net::socket_stream::<Socket>(
                options.uplink_address,
                DEFAULT_BACKOFF.clone(),
                SocketMode::Connect,
            )
            .pipe(stream_unwrap!("connecting to uplink socket"));

            let (uplink, uplink_pump) = relay::uplink_stream(uplink_sockets)
                .await
                .pipe(|s| {
                    let tripwire = tripwire.clone();

                    futures::stream::StreamExt::take_until(s, async move {
                        let tripwire = tripwire.clone();
                        tripwire.recv().await.unwrap_err();
                        tracing::debug!("uplink reading complete");
                    })
                })
                .pipe(|s| splittable_stream(s, 1024));

            // TODO: handle cobs
            let (read_packets, pump_serial_reader) = reader
                .pipe(|r| io::split_packets(smol::io::BufReader::new(r), 0, 8192))
                .pipe(stream_unwrap!("splitting serial packet"))
                .pipe(|s| io::unpack_cobs_stream(s, 0))
                .pipe(stream_unwrap!("unwrapping cobs"))
                .pipe(|s| {
                    let tripwire = tripwire.clone();

                    futures::stream::StreamExt::take_until(s, async move {
                        let tripwire = tripwire.clone();
                        tripwire.recv().await.unwrap_err();
                        tracing::debug!("serial packet reading complete");
                    })
                })
                .pipe(|s| splittable_stream(s, 1024));

            let (cseq, drive_serial) = relay::assemble_serial(read_packets.clone(), writer);

            let relay_uplink = relay::relay_uplink_to_serial(
                uplink.clone(),
                cseq,
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
                relay::dummy_log_downlink(trace_event_stream).pipe(move |s| {
                    let tripwire = tripwire;

                    futures::stream::StreamExt::take_until(
                        s,
                        Box::pin(async move {
                            tripwire.recv().await.unwrap_err();
                            tracing::debug!("trace downlink done");
                        }),
                    )
                }),
                serial_relay,
            );

            let downlink_sockets = options.downlink_addresses.iter().cloned().map(|addr| {
                net::socket_stream::<Socket>(addr, DEFAULT_BACKOFF.clone(), SocketMode::Connect)
                    .pipe(stream_unwrap!("connecting to downlink socket"))
            });

            let downlink = relay::send_downlink::<Socket>(downlink_packets, downlink_sockets);

            futures::future::join5(
                drive_serial.instrument(tracing::debug_span!("serial drive")),
                pump_serial_reader.instrument(tracing::debug_span!("serial pump")),
                relay_uplink.instrument(tracing::debug_span!("uplink to serial relay")),
                uplink_pump.instrument(tracing::debug_span!("uplink pump")),
                downlink.instrument(tracing::debug_span!("downlink")),
            )
            .await;

            tracing::debug!("done");

            Ok(())
        }
    })
}
