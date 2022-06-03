#![feature(never_type)]
#![feature(try_blocks)]

use crate::common::serial_ack_backend;
use lunarrelay::{
    message::{
        header::{
            Const,
            Conversation,
            Destination,
            Disposition,
            RequestMeta,
            Server,
        },
        Header,
        Message,
    },
    util,
};

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

    let Message {
        header,
        payload,
    } = util::timeout(500, harness.csq.submit(&orig_msg)).await??;
    let ack = payload.as_ref();

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

    harness.done.close();

    let (_, ser) = util::timeout(1000, smol::future::zip(pump_task, serial_task)).await?;
    ser?;

    Ok(())
}
