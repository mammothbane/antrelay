use std::sync::Once;

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
    Message,
    SourceInfo,
};

pub mod ant_decode;
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
pub struct AckMessage(pub Message);

#[derive(Clone, Debug, PartialEq, Message, derive_more::Into, derive_more::AsRef)]
#[rtype(result = "()")]
pub struct AntMessage(pub Message);

#[tracing::instrument(skip_all, fields(%msg))]
fn try_issue_ack<A>(issue: &A, msg: &Message, ctx: &mut A::Context)
where
    A: Actor + BrokerIssue,
    A::Context: AsyncContext<A>,
{
    let inner = msg.as_ref();

    let (unique_id, crc) = match inner.header.payload {
        SourceInfo::Info(info) => {
            let header = info.header;
            (header.unique_id(), info.checksum)
        },
        SourceInfo::Empty => {
            tracing::trace!("message has no source");
            return;
        },
    };

    tracing::info!(unique_id = %inner.header.header.unique_id(), response_to = ?unique_id, orig_crc = ?crc, "decoded acked command");
    issue.issue_sync::<SystemBroker, _>(AckMessage(msg.clone()), ctx);
}

pub struct Serial {
    subscribe_once: Once,
}

impl Default for Serial {
    fn default() -> Self {
        Self {
            subscribe_once: Once::new(),
        }
    }
}

impl Actor for Serial {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_once.call_once(|| {
            self.subscribe_async::<SystemBroker, UpMessage>(ctx);
            self.subscribe_async::<SystemBroker, raw::DownPacket>(ctx);
        });
    }
}

impl Supervised for Serial {}

impl Handler<UpMessage> for Serial {
    type Result = ();

    #[tracing::instrument(skip_all, fields(msg = %msg.0))]
    fn handle(&mut self, msg: UpMessage, ctx: &mut Self::Context) -> Self::Result {
        use actix_broker::BrokerIssue;

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

        let _span =
            tracing::info_span!("serial message decoded", event = ?inner.header.header.ty.event)
                .entered();

        try_issue_ack(self, &unpacked, ctx);
    }
}
