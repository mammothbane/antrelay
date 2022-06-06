#![feature(never_type)]
#![feature(try_blocks)]

use futures::StreamExt;

use antrelay::{
    io::unpack_cobs,
    message::{
        header::{
            Const,
            Conversation,
            Destination,
            Disposition,
            RequestMeta,
            Server,
        },
        payload::{
            Ack,
            RealtimeStatus,
        },
        Header,
        HeaderPacket,
        Message,
        OpaqueBytes,
    },
    util,
    util::{
        pack_message,
        unpack_message,
    },
};

use crate::common::serial_ack_backend;

mod common;

#[async_std::test]
async fn test_5v_sup() -> eyre::Result<()> {
    common::trace_init();

    let mut harness = common::construct_graph();

    harness.uplink.close();

    let pumps = harness.pumps.take().unwrap();
    let pump_task = smol::spawn(pumps);

    let serial_task = smol::spawn(serial_ack_backend(
        harness.serial_read,
        harness.serial_write,
        harness.done_rx.clone(),
    ));

    let orig_msg = Message::new(
        Header::cs_command::<Const<0>>(0, Conversation::VoltageSupplied),
        Vec::<u8>::new(),
    );

    let ack_msg: Message<Ack> = util::timeout(500, harness.csq.submit(orig_msg.clone())).await??;

    let payload = &ack_msg.payload;
    let header = &ack_msg.header;

    let ack: &Ack = payload.as_ref();

    assert_eq!(ack.checksum, orig_msg.payload.checksum()?[0]);
    assert_eq!(ack.seq, orig_msg.header.seq);
    assert_eq!(ack.timestamp, orig_msg.header.timestamp);

    assert_eq!(header.destination, Destination::Frontend);
    assert_eq!(header.ty, RequestMeta {
        disposition:         Disposition::Response,
        request_was_invalid: false,
        server:              Server::CentralStation,
        conversation_type:   Conversation::VoltageSupplied,
    });

    let pkt = harness.downlink.next().await.ok_or(eyre::eyre!("awaiting downlink result"))?;
    let msg = unpack_message::<HeaderPacket<RealtimeStatus, OpaqueBytes>>(pkt)?;

    assert_eq!(msg.header.ty.conversation_type, Conversation::Relay);
    assert_eq!(msg.header.destination, Destination::Ground);
    assert_eq!(msg.header.ty.server, Server::Frontend);

    let inner = msg.payload.as_ref();

    let packed_ack = pack_message(ack_msg.clone())?;
    assert_eq!(unpack_cobs(inner.payload.to_vec(), 0)?, &packed_ack[..]);

    harness.done.close();

    let (_, ser) = util::timeout(1000, smol::future::zip(pump_task, serial_task)).await?;
    ser?;

    Ok(())
}
