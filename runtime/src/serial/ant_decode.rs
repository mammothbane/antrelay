use std::sync::Once;

use actix::{
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
use packed_struct::PackedStructSlice;

use crate::serial::{
    AntMessage,
    DownMessage,
};
use message::{
    header::{
        Disposition,
        Event,
    },
    Message,
};

pub struct AntDecode {
    subscribe_once: Once,
}

impl Default for AntDecode {
    fn default() -> Self {
        Self {
            subscribe_once: Once::new(),
        }
    }
}

impl Actor for AntDecode {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_once.call_once(|| {
            self.subscribe_async::<SystemBroker, DownMessage>(ctx);
        });
    }
}

impl Supervised for AntDecode {}

impl Handler<DownMessage> for AntDecode {
    type Result = ();

    #[tracing::instrument(skip_all, fields(%msg))]
    fn handle(&mut self, DownMessage(msg): DownMessage, ctx: &mut Self::Context) -> Self::Result {
        let msg = msg.as_ref();

        if !matches!(msg.header.header.ty, message::header::MessageType {
            event: Event::CSRelay,
            disposition: Disposition::Ack,
            ..
        }) {
            return;
        }

        let ant_bytes = msg.payload.as_ref().slice(20..);

        let ant_msg = match <Message as PackedStructSlice>::unpack_from_slice(&ant_bytes) {
            Ok(x) => x,
            Err(e) => {
                tracing::error!(error = %e, data = %hex::encode(msg.payload.as_ref()), "failed to unpack ant message");
                return;
            },
        };

        tracing::info!(%ant_msg, "decoded ant message");

        super::try_issue_ack(self, &ant_msg);
        self.issue_async::<SystemBroker, _>(AntMessage(ant_msg));
    }
}
