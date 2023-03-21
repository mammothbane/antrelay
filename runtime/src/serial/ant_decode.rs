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
use message::header::{
    Disposition,
    Event,
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
    fn handle(&mut self, DownMessage(msg): DownMessage, _ctx: &mut Self::Context) -> Self::Result {
        let msg = msg.as_ref();

        if !matches!(msg.header.header.ty, message::header::MessageType {
            event: Event::CSRelay,
            disposition: Disposition::Ack,
            ..
        }) {
            return;
        }

        let relay = match <message::CSRelay as PackedStructSlice>::unpack_from_slice(
            msg.payload.as_ref(),
        ) {
            Ok(x) => x,
            Err(e) => {
                tracing::error!(error = %e, "failed to unpack ant message");
                return;
            },
        };

        let cs = &relay.header;
        let ant_hdr = &relay.payload.header;
        let ant = &relay.payload.payload;

        tracing::info!(cs = %cs.display(), %ant_hdr, ant = %ant.display(), limit_downlink = true, "decoded relay message");
        self.issue_system_async(AntMessage(relay.payload));
    }
}
