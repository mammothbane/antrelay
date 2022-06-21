#![feature(never_type)]
#![feature(try_blocks)]
#![feature(explicit_generic_args_with_impl_trait)]

use futures::StreamExt;
use tap::Pipe;

use antrelay::message::{
    header::{
        Conversation,
        Destination,
        Disposition,
        RequestMeta,
        Server,
    },
    payload::{
        Ack,
        RelayPacket,
    },
    CRCMessage,
    CRCWrap,
    Header,
    StandardCRC,
};

use common::Harness;

use crate::common::serial_ack_backend;

mod common;

#[async_std::test]
async fn test_5v_sup() -> eyre::Result<()> {
    common::trace_init();

    let mut harness = Harness::new();

    let pumps = harness.pumps.take().unwrap();
    let pump_task = smol::spawn(pumps);

    let serial_task = smol::spawn(serial_ack_backend(
        harness.serial_codec.clone(),
        harness.serial_read,
        harness.serial_write,
        harness.done_rx.clone(),
    ));

    let orig_msg = CRCMessage::<Vec<u8>, StandardCRC>::new(
        Header::fe_command(&harness.packet_env, Conversation::PowerSupplied),
        vec![],
    );

    (harness.link_codecs.uplink.encode)(orig_msg.clone())?
        .pipe(|pkt| harness.uplink.broadcast(pkt))
        .await?;

    tracing::debug!("sent uplink command");

    let pkt = (&mut harness.downlink)
        .map(|x| (harness.link_codecs.downlink.decode)(x))
        .filter(|msg| {
            return smol::future::ready(true);

            let result = match msg {
                Err(_) => true,
                Ok(msg) => msg.header.ty.server == Server::CentralStation,
            };

            smol::future::ready(result)
        })
        .next()
        .await
        .ok_or(eyre::eyre!("awaiting downlink result"))??;

    tracing::debug!(%pkt, "got downlinked packet");

    let msg = pkt.payload_into::<CRCWrap<RelayPacket>>()?;
    tracing::debug!("converted packet");

    assert_eq!(msg.header.ty.conversation_type, Conversation::Relay);
    assert_eq!(msg.header.destination, Destination::Ground);
    assert_eq!(msg.header.ty.server, Server::CentralStation);

    let inner = msg.payload.take();

    let serial_msg = (harness.serial_codec.decode)(inner.payload)?;
    tracing::debug!("deserialized wrapped serial packet");

    let ack_msg = serial_msg.payload_into::<CRCWrap<Ack>>()?;
    tracing::debug!("into ack");

    let payload = &ack_msg.payload;
    let header = &ack_msg.header;

    let ack: &Ack = payload.as_ref();

    assert_eq!(ack.checksum, orig_msg.payload.checksum()?[0]);

    assert_eq!(header.destination, Destination::Frontend);
    assert_eq!(header.ty, RequestMeta {
        disposition:         Disposition::Ack,
        request_was_invalid: false,
        server:              Server::CentralStation,
        conversation_type:   Conversation::Ping,
    });

    harness.done.close();
    assert_eq!(harness.downlink.next().await, None);
    smol::future::zip(pump_task, serial_task).await.1?;

    Ok(())
}
