#![feature(never_type)]
#![feature(try_blocks)]
#![feature(explicit_generic_args_with_impl_trait)]

use futures::StreamExt;

use antrelay::{
    io::unpack_cobs,
    message::{
        header::{
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
    util::{
        self,
        pack_message,
    },
};

use common::Harness;

use crate::common::serial_ack_backend;

mod common;

#[async_std::test]
async fn test_5v_sup() -> eyre::Result<()> {
    common::trace_init();

    let mut harness = Harness::new();

    harness.uplink.close();

    let pumps = harness.pumps.take().unwrap();
    let pump_task = smol::spawn(pumps);

    let serial_task = smol::spawn(serial_ack_backend(
        harness.serial_codec.clone(),
        harness.serial_read,
        harness.serial_write,
        harness.done_rx.clone(),
    ));

    let orig_msg = Message::new(
        Header::cs_command(&harness.packet_env, Conversation::VoltageSupplied),
        Vec::<u8>::new(),
    );

    let ack_msg: Message<Ack> = util::timeout(100, harness.csq.submit(orig_msg.clone())).await??;

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

    let msg = (harness.link_codec.deserialize)(pkt)?
        .payload_into::<HeaderPacket<RealtimeStatus, OpaqueBytes>>()?;

    assert_eq!(msg.header.ty.conversation_type, Conversation::Relay);
    assert_eq!(msg.header.destination, Destination::Ground);
    assert_eq!(msg.header.ty.server, Server::Frontend);

    let inner = msg.payload;

    let packed_ack = {
        let mut result = pack_message(ack_msg.clone())?;
        result.push(0);
        result
    };
    assert_eq!(unpack_cobs(inner.payload.to_vec(), 0)?, &packed_ack[..]);

    harness.done.close();

    let down_next = util::timeout(100, harness.downlink.next()).await?;
    assert_eq!(down_next, None);

    let (_, ser) = util::timeout(100, smol::future::zip(pump_task, serial_task)).await?;
    ser?;

    Ok(())
}
