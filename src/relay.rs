use std::error::Error;

use async_std::prelude::Stream;
use backoff::backoff::Backoff;
use eyre::WrapErr;
use futures::AsyncWrite;
use packed_struct::PackedStructSlice;
use smol::{
    io::AsyncRead,
    stream::StreamExt,
};
use tap::Pipe;

use crate::{
    message::{
        crc_wrap::Ack,
        header::{
            Destination,
            Kind,
            Target,
            Type,
        },
        CRCWrap,
        Header,
        Message,
        OpaqueBytes,
    },
    net::{
        receive_packets,
        DatagramReceiver,
        DatagramSender,
    },
    packet_io::PacketIO,
    util::{
        self,
        deserialize_messages,
        log_and_discard_errors,
        splittable_stream,
    },
    MissionEpoch,
};

pub async fn assemble_downlink<Socket, R, W>(
    uplink: impl Stream<Item = Message<OpaqueBytes>> + Unpin + Clone + 'static,
    all_serial_packets: impl Stream<Item = Message<OpaqueBytes>> + Unpin + 'static,
    log_messages: impl Stream<Item = Message<OpaqueBytes>> + Unpin + 'static,
    packet_io: &'static PacketIO<R, W>,
    serial_backoff: impl Backoff + Clone + 'static,
) -> impl Stream<Item = Vec<u8>>
where
    Socket: DatagramReceiver + DatagramSender,
    Socket::Error: Error + Send + Sync + 'static,
    R: AsyncRead + 'static,
    W: AsyncWrite + Unpin + 'static,
{
    let serial_acks = relay_uplink_to_serial(uplink.clone(), &packet_io, serial_backoff)
        .await
        .map(|msg| msg.payload_into::<CRCWrap<OpaqueBytes>>())
        .pipe(|s| log_and_discard_errors(s, "packing ack for downlink"));

    uplink
        .race(all_serial_packets)
        .race(log_messages)
        .race(serial_acks)
        .map(|msg| msg.pack_to_vec())
        .pipe(|s| log_and_discard_errors(s, "packing message for downlink"))
        .map(|mut data| util::brotli_compress(&mut data))
        .pipe(|s| log_and_discard_errors(s, "compressing message for downlink"))
}

pub async fn uplink_stream<Socket>(
    address: Socket::Address,
    backoff: impl Backoff + Clone + Send + Sync + 'static,
    buffer: usize,
) -> impl Stream<Item = Message<OpaqueBytes>>
where
    Socket: DatagramReceiver + Send + Sync + 'static,
    Socket::Address: Send + Clone + Sync,
    Socket::Error: Error + Send + Sync + 'static,
{
    receive_packets::<Socket>(address, backoff)
        .pipe(deserialize_messages)
        .pipe(|s| log_and_discard_errors(s, "deserializing messages"))
        .pipe(|s| splittable_stream(s, buffer))
}

pub async fn relay_graph<Socket>(
    done: smol::channel::Receiver<!>,
    serial_read: impl AsyncRead + Unpin + 'static,
    serial_write: impl AsyncWrite + Unpin + 'static,
    serial_request_backoff: impl Backoff + Clone + 'static,
    uplink: impl Stream<Item = Message<OpaqueBytes>> + Send + 'static,
    log_stream: impl Stream<Item = crate::tracing::Event> + Unpin + 'static,
) -> impl Stream<Item = Vec<u8>> + Unpin
where
    Socket: DatagramReceiver + DatagramSender,
    Socket::Error: Error + Send + Sync + 'static,
{
    let packet_io = PacketIO::new(smol::io::BufReader::new(serial_read), serial_write, done);
    let packet_io = Box::leak(Box::new(packet_io));

    let all_serial = packet_io.read_packets(0u8).await;

    let uplink_split = splittable_stream(uplink, 1024);

    assemble_downlink::<Socket, _, _>(
        uplink_split,
        all_serial,
        log_stream.pipe(dummy_log_downlink),
        packet_io,
        serial_request_backoff,
    )
    .await
}

#[tracing::instrument]
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

pub async fn send_downlink<Socket>(
    packets: impl Stream<Item = Vec<u8>> + Unpin + Send + 'static,
    addresses: impl IntoIterator<Item = Socket::Address>,
    backoff: impl Backoff + Clone,
) where
    Socket: DatagramSender,
    Socket::Error: Error,
{
    {
        let split = splittable_stream(packets, 1024);

        addresses.into_iter().map(move |addr| {
            crate::net::send_packets::<Socket>(addr, split.clone(), backoff.clone())
        })
    }
    .pipe(futures::future::join_all)
    .await;
}

pub async fn relay_uplink_to_serial<'a, 'u, R, W>(
    uplink: impl Stream<Item = Message<OpaqueBytes>> + 'u,
    packetio: &'a PacketIO<R, W>,
    request_backoff: impl Backoff + Clone + 'static,
) -> impl Stream<Item = Message<Ack>> + 'a
where
    W: AsyncWrite + Unpin,
    'u: 'a,
{
    uplink
        .filter(|msg| msg.header.destination != crate::message::header::Destination::Frontend)
        .map(|msg| -> eyre::Result<_> {
            let crc = msg.payload.checksum()?.to_vec();

            Ok((msg, crc))
        })
        .pipe(|s| log_and_discard_errors(s, "computing incoming message checksum"))
        .then(move |(msg, crc): (Message<OpaqueBytes>, Vec<u8>)| {
            Box::pin(backoff::future::retry_notify(
                request_backoff.clone(),
                move || {
                    let msg = msg.clone();
                    let crc = crc.clone();

                    async move {
                        let g = packetio.request(&msg).await.wrap_err("sending serial request")?;
                        let ret = g.wait().await.wrap_err("receiving serial response")?;

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
        .pipe(|s| log_and_discard_errors(s, "no ack from serial connection"))
}

// TODO: use actual log format
fn dummy_log_downlink(
    s: impl Stream<Item = crate::tracing::Event>,
) -> impl Stream<Item = Message<OpaqueBytes>> {
    s.pipe(|s| futures::stream::StreamExt::chunks(s, 5)).map(|evts| {
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
    })
}
