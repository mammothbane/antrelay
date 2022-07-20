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
use packed_struct::PackedStructSlice;

use message::{
    header::{
        Event,
        MessageType,
    },
    Message,
};

mod commander;
pub mod raw;

pub use commander::*;

#[derive(Clone, Debug, PartialEq, Message, derive_more::Into, derive_more::AsRef)]
#[rtype(result = "()")]
pub struct UpMessage(pub Message);

#[derive(Clone, Debug, PartialEq, Message, derive_more::Into, derive_more::AsRef)]
#[rtype(result = "()")]
pub struct DownMessage(pub Message);

#[derive(Clone, Debug, PartialEq, Message, derive_more::Into, derive_more::AsRef)]
#[rtype(result = "()")]
pub struct AckMessage(pub message::Ack);

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

    #[tracing::instrument(skip_all, fields(msg = %msg.0))]
    fn handle(&mut self, msg: UpMessage, ctx: &mut Self::Context) -> Self::Result {
        tracing::info!("sending serial command");

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
        let unpacked = match <Message as PackedStructSlice>::unpack_from_slice(msg.0.as_ref()) {
            Ok(dl) => dl,
            Err(e) => {
                tracing::error!(error = %e, "unpacking serial downlink message");
                return;
            },
        };

        self.issue_sync::<SystemBroker, _>(DownMessage(unpacked.clone()), ctx);

        let inner = unpacked.as_ref();
        let header = &inner.header;

        let _span =
            tracing::info_span!("serial message decoded", event = ?header.ty.event).entered();

        match header.ty {
            MessageType {
                event: Event::CSPing | Event::CSRelay,
                ..
            } => {
                tracing::debug!(payload = %hex::encode(&*inner.payload.as_ref()), "serial message is a relay/ping packet")
            },
            _ => {
                let ack = match inner.payload_into::<Message>() {
                    Ok(ack) => Message::new(ack),
                    Err(e) => {
                        tracing::warn!(error = %e, "interpreting payload as ack message");
                        return;
                    },
                };

                tracing::info!(unique_id = %ack.as_ref().header.unique_id(), "decoded acked serial command");

                self.issue_sync::<SystemBroker, _>(AckMessage(ack), ctx);
            },
        }
    }
}
