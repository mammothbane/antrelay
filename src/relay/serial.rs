use std::{
    cell::RefCell,
    rc::Rc,
};

use async_std::prelude::Stream;
use backoff::backoff::Backoff;
use eyre::WrapErr;
use futures::{
    AsyncRead,
    AsyncWrite,
};
use smol::prelude::*;
use tap::Pipe;
use tracing::Instrument;

use crate::{
    io::{
        self,
        CommandSequencer,
    },
    message::{
        header::{
            Destination,
            Kind,
            Source,
            Type,
        },
        payload::{
            realtime_status::RealtimeStatus,
            Ack,
            RelayPacket,
        },
        CRCWrap,
        Header,
        Message,
        OpaqueBytes,
    },
    stream_unwrap,
    MissionEpoch,
};

#[tracing::instrument(level = "debug")]
pub async fn connect_serial(
    path: String,
    baud: u32,
) -> eyre::Result<(impl AsyncRead + Unpin, impl AsyncWrite + Unpin)> {
    let builder = tokio_serial::new(&path, baud);

    let stream =
        async_compat::Compat::new(async { tokio_serial::SerialStream::open(&builder) }).await?;

    let stream = async_compat::Compat::new(stream);
    Ok(smol::io::split(stream))
}

#[tracing::instrument(skip_all, level = "debug")]
pub fn assemble_serial(
    reader: impl Stream<Item = Vec<u8>> + Unpin,
    writer: impl AsyncWrite + Unpin + 'static,
) -> (CommandSequencer, impl Future<Output = ()>) {
    let (cseq, serial_responses, serial_requests) = CommandSequencer::new(reader);

    let serial_requests = serial_requests.pipe(|s| io::pack_cobs_stream(s, 0));

    let pump_serial_writer = io::write_packet_stream(serial_requests, writer)
        .pipe(stream_unwrap!("writing packets to serial"))
        .for_each(|_| {})
        .instrument(tracing::debug_span!("writing packets to serial"));

    let packet_handler = serial_responses
        .pipe(stream_unwrap!("reading from serial"))
        .for_each(|_| {})
        .instrument(tracing::debug_span!("reading responses from serial"));

    let drive_all = async move {
        // futures::future::join(pump_serial_writer, packet_handler).await;
        smol::future::zip(pump_serial_writer, packet_handler).await;
        // pump_serial_writer.await;

        // packet_handler.await;
    };

    (cseq, drive_all)
}

#[tracing::instrument(skip_all)]
pub fn relay_uplink_to_serial(
    uplink: impl Stream<Item = Message<OpaqueBytes>>,
    csq: CommandSequencer,
    request_backoff: impl Backoff + Clone + 'static,
) -> impl Stream<Item = Message<Ack>> {
    let csq = Rc::new(RefCell::new(csq));

    uplink
        .filter(|msg| msg.header.destination != Destination::Frontend)
        .map(|msg| -> eyre::Result<_> {
            let crc = msg.payload.checksum()?.to_vec();

            Ok((msg, crc))
        })
        .pipe(stream_unwrap!("computing incoming message checksum"))
        .then(move |(msg, crc): (Message<OpaqueBytes>, Vec<u8>)| {
            let csq = &csq;
            let csq = csq.clone();

            Box::pin(backoff::future::retry_notify(
                request_backoff.clone(),
                move || {
                    let msg = msg.clone();
                    let crc = crc.clone();
                    let csq = csq.clone();

                    async move {
                        let csq = csq.as_ref().borrow();
                        let ret = csq.submit(&msg).await.wrap_err("sending command to serial")?;

                        if ret.header.ty.acked_message_invalid {
                            return Err(backoff::Error::transient(eyre::eyre!(
                                "ack message had invalid bit"
                            )));
                        }

                        if &[ret.payload.as_ref().checksum] != crc.as_slice() {
                            return Err(backoff::Error::transient(eyre::eyre!(
                                "ack message without invalid mismatch bit but mismatching checksum"
                            )));
                        }

                        Ok(ret) as Result<Message<Ack>, backoff::Error<eyre::Report>>
                    }
                },
                |e, dur| {
                    tracing::error!(error = %e, backoff_dur = ?dur, "retrieving from serial");
                },
            ))
        })
        .pipe(stream_unwrap!("no ack from serial connection"))
}

#[inline]
#[tracing::instrument(skip_all)]
pub fn wrap_relay_packets(
    packets: impl Stream<Item = Vec<u8>>,
    status: impl Stream<Item = RealtimeStatus>,
) -> impl Stream<Item = Message<RelayPacket>> {
    packets.zip(status).map(|(packet, status)| Message {
        header:  Header {
            magic:       Default::default(),
            destination: Destination::Ground,
            timestamp:   MissionEpoch::now(),
            seq:         0, // TODO
            ty:          Type {
                ack:                   false,
                acked_message_invalid: false,
                source:                Source::Frontend,
                kind:                  Kind::Relay,
            },
        },
        payload: CRCWrap::new(RelayPacket {
            header:  status,
            payload: packet,
        }),
    })
}
