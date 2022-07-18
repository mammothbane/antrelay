use actix::prelude::*;
use actix_broker::{
    BrokerIssue,
    BrokerSubscribe,
    SystemBroker,
};
use std::time::Duration;
use tokio::sync::oneshot;

use crate::serial;
use message::{
    payload::Ack,
    BytesWrap,
    Message,
    UniqueId,
};

#[derive(Debug, Clone, PartialEq, Message)]
#[rtype(result = "Option<Message<Ack>>")]
pub struct Request(message::Message<BytesWrap>);

#[derive(Message)]
#[rtype(result = "()")]
pub struct GC;

#[derive(Default)]
pub struct Commander {
    requests: fnv::FnvHashMap<UniqueId, oneshot::Sender<Message<Ack>>>,
}

impl Actor for Commander {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_sync::<SystemBroker, serial::AckMessage>(ctx);

        ctx.run_interval(Duration::from_secs(5), |a, _ctx| {
            let keys = a
                .requests
                .iter()
                .filter(|(_, v)| v.is_closed())
                .map(|(id, _)| *id)
                .collect::<Vec<_>>();

            keys.into_iter().for_each(|k| {
                a.requests.remove(&k);
            });
        });
    }
}

impl Supervised for Commander {}
impl SystemService for Commander {}

impl Handler<Request> for Commander {
    type Result = ResponseFuture<Option<Message<Ack>>>;

    fn handle(&mut self, msg: Request, ctx: &mut Self::Context) -> Self::Result {
        let (tx, rx) = oneshot::channel();
        self.requests.insert(msg.0.as_ref().header.unique_id(), tx);
        self.issue_sync::<SystemBroker, _>(serial::DownMessage(msg.0), ctx);

        Box::pin(async move { rx.await.ok() })
    }
}

impl Handler<serial::AckMessage> for Commander {
    type Result = ();

    fn handle(&mut self, msg: serial::AckMessage, _ctx: &mut Self::Context) -> Self::Result {
        let msg_id = msg.0.as_ref().header.unique_id();

        match self.requests.remove(&msg_id) {
            Some(tx) => {
                if let Err(_msg) = tx.send(msg.0) {
                    tracing::warn!(?msg_id, "tried to ack command, listener disappeared");
                }
            },
            None => {
                tracing::warn!(?msg_id, "received ack message for unregistered command");
            },
        }
    }
}
