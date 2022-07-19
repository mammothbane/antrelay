use actix::{
    prelude::*,
    Actor,
    Context,
    Handler,
    Supervised,
};
use actix_broker::{
    BrokerIssue,
    BrokerSubscribe,
    SystemBroker,
};
use bytes::BytesMut;
use message::{
    header::Disposition,
    payload,
    BytesWrap,
    Message,
};
use packed_struct::PackedStructSlice;

mod commander;
pub mod raw;

pub use commander::*;

#[derive(Clone, Debug, PartialEq, Message)]
#[rtype(result = "()")]
pub struct UpMessage(pub Message<BytesWrap>);

#[derive(Clone, Debug, PartialEq, Message)]
#[rtype(result = "()")]
pub struct DownMessage(pub Message<BytesWrap>);

#[derive(Clone, Debug, PartialEq, Message)]
#[rtype(result = "()")]
pub struct AckMessage(pub Message<payload::Ack>);

pub struct Serial;

impl Actor for Serial {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_async::<SystemBroker, UpMessage>(ctx);
        self.subscribe_async::<SystemBroker, raw::DownPacket>(ctx);
    }
}

impl Supervised for Serial {}

impl Handler<UpMessage> for Serial {
    type Result = ();

    fn handle(&mut self, msg: UpMessage, ctx: &mut Self::Context) -> Self::Result {
        let result = match msg.0.pack_to_vec() {
            Ok(v) => BytesMut::from(&v[..]).freeze(),
            Err(e) => {
                tracing::error!(error = %e, "packing serial uplink message");
                return;
            },
        };

        self.issue_sync::<SystemBroker, _>(raw::UpPacket(result), ctx);
    }
}

impl Handler<raw::DownPacket> for Serial {
    type Result = ();

    fn handle(&mut self, msg: raw::DownPacket, ctx: &mut Self::Context) -> Self::Result {
        let unpacked =
            match <Message<BytesWrap> as PackedStructSlice>::unpack_from_slice(msg.0.as_ref()) {
                Ok(dl) => dl,
                Err(e) => {
                    tracing::error!(error = %e, "unpacking serial downlink message");
                    return;
                },
            };

        if unpacked.as_ref().header.ty.disposition == Disposition::Ack {
            let inner = unpacked.as_ref();

            let ack = match inner.payload_into::<payload::Ack>() {
                Ok(ack) => Message::new(ack),
                Err(e) => {
                    tracing::error!(error = %e, "reinterpreting payload as ack message");
                    return;
                },
            };

            self.issue_sync::<SystemBroker, _>(AckMessage(ack), ctx);
        }

        self.issue_sync::<SystemBroker, _>(DownMessage(unpacked), ctx);
    }
}
