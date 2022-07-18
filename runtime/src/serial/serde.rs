use actix::prelude::*;
use actix_broker::{
    BrokerIssue,
    BrokerSubscribe,
    SystemBroker,
};
use bytes::BytesMut;
use packed_struct::PackedStructSlice;

use message::{
    header::Disposition,
    payload,
    BytesWrap,
    Message,
};

use crate::serial::raw;

#[derive(Clone, Debug, PartialEq, Message)]
#[rtype(result = "()")]
pub struct UpPacket(pub Message<BytesWrap>);

#[derive(Clone, Debug, PartialEq, Message)]
#[rtype(result = "()")]
pub struct DownPacket(pub Message<BytesWrap>);

#[derive(Clone, Debug, PartialEq, Message)]
#[rtype(result = "()")]
pub struct AckPacket(pub Message<payload::Ack>);

pub struct Serde;

impl Actor for Serde {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_async::<SystemBroker, UpPacket>(ctx);
        self.subscribe_async::<SystemBroker, raw::DownPacket>(ctx);
    }
}

impl Supervised for Serde {}

impl Handler<UpPacket> for Serde {
    type Result = ();

    fn handle(&mut self, msg: UpPacket, ctx: &mut Self::Context) -> Self::Result {
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

impl Handler<raw::DownPacket> for Serde {
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

            self.issue_sync::<SystemBroker, _>(AckPacket(ack), ctx);
        }

        self.issue_sync::<SystemBroker, _>(DownPacket(unpacked), ctx);
    }
}
