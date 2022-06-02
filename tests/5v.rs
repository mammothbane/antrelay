use lunarrelay::message::{
    header::{
        Const,
        Kind,
    },
    Header,
    Message,
};
use smol::stream::StreamExt;
use lunarrelay::message::header::{Destination, Source, Type};

mod common;

#[async_std::test]
async fn test_5v_sup() -> eyre::Result<()> {
    let harness = common::construct_graph();

    let t = smol::spawn(harness.drive_serial.for_each(|_| {}));

    let orig_msg =
        Message::new(Header::cs_command::<Const<0>>(0, Kind::VoltageSupplied), Vec::<u8>::new());

    let resp = harness.packet_io.request(&orig_msg).await?;

    let Message {
        header,
        payload,
    } = resp.wait().await?;

    let ack = payload.as_ref();

    assert_eq!(ack.checksum, orig_msg.payload.checksum()?[0]);
    assert_eq!(ack.seq, orig_msg.header.seq);
    assert_eq!(ack.timestamp, orig_msg.header.timestamp);

    assert_eq!(header.destination, Destination::Ground);
    assert_eq!(header.ty, Type {
        ack: true,
        acked_message_invalid: false,
        source: Source::CentralStation,
        kind: Kind::VoltageSupplied
    });
    assert_eq!(header.timestamp, )

    t.cancel().await;

    Ok(())
}
