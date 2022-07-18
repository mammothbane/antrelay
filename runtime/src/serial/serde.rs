use actix::prelude::*;
use actix_broker::{
    BrokerIssue,
    BrokerSubscribe,
    SystemBroker,
};
use bytes::{
    Bytes,
    BytesMut,
};
use packed_struct::PackedStructSlice;

use message::{
    header::Disposition,
    payload,
    Message,
};

use crate::serial::raw;

#[derive(Clone, Debug, PartialEq, Hash, Message)]
#[rtype(result = "()")]
pub struct Uplink(pub Message<Bytes>);

#[derive(Clone, Debug, PartialEq, Hash, Message)]
#[rtype(result = "()")]
pub struct Downlink(pub Message<Bytes>);

#[derive(Clone, Debug, PartialEq, Hash, Message)]
#[rtype(result = "()")]
pub struct AckDownlink(pub Message<payload::Ack>);

pub struct Serde;

impl Actor for Serde {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_async::<SystemBroker, Uplink>(ctx);
        self.subscribe_async::<SystemBroker, raw::Downlink>(ctx);
    }
}

impl Supervised for Serde {}

impl Handler<Uplink> for Serde {
    type Result = ();

    fn handle(&mut self, msg: Uplink, _ctx: &mut Self::Context) -> Self::Result {
        let result = match msg.pack_to_vec() {
            Ok(v) => BytesMut::from(&v[..]).freeze(),
            Err(e) => {
                tracing::error!(error = %e, "packing serial uplink message");
                return;
            },
        };

        self.issue_sync::<SystemBroker, _>(raw::Uplink(result));
    }
}

impl Handler<raw::Downlink> for Serde {
    type Result = ();

    fn handle(&mut self, msg: raw::Downlink, _ctx: &mut Self::Context) -> Self::Result {
        let downlink = match Downlink::unpack_from_slice(msg.0.as_ref()) {
            Ok(dl) => dl,
            Err(e) => {
                tracing::error!(error = %e, "unpacking serial downlink message");
                return;
            },
        };

        self.issue_sync::<SystemBroker, _>(downlink);

        if downlink.0.as_ref().header.ty.disposition == Disposition::Ack {
            let inner = downlink.0.as_ref();

            let ack = match inner.payload_into::<payload::Ack>() {
                Ok(ack) => Message::new(ack),
                Err(e) => {
                    tracing::error!(error = %e, "reinterpreting payload as ack message");
                    return;
                },
            };

            self.issue_sync::<SystemBroker, _>(AckDownlink(ack));
        }
    }
}
