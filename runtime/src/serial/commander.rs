use std::time::Duration;

use actix::prelude::*;
use actix_broker::{
    BrokerIssue,
    BrokerSubscribe,
    SystemBroker,
};
use futures::future::BoxFuture;
use tokio::sync::oneshot;

use message::UniqueId;

use crate::{
    serial,
    OverrideRegistry,
};

#[derive(Debug, Clone, PartialEq, Message)]
#[rtype(result = "Option<message::Ack>")]
pub struct Request(pub message::Message);

#[derive(Default)]
pub struct Commander {
    requests: fnv::FnvHashMap<UniqueId, oneshot::Sender<message::Ack>>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Mailbox(#[from] MailboxError),

    #[error("request was dropped")]
    RequestDropped,

    #[error("invalid response checksum")]
    InvalidChecksum,
}

pub async fn send_retry(
    mut message: impl FnMut() -> BoxFuture<'static, message::Message>,
    request_timeout: Duration,
    retry_strategy: impl IntoIterator<Item = Duration>,
) -> Result<message::Ack, Error> {
    tokio_retry::Retry::spawn(retry_strategy, || {
        let message = message();

        async move {
            let msg = message.await;
            send(msg, Some(request_timeout)).await
        }
    })
    .await
}

#[inline]
pub async fn send(
    message: message::Message,
    timeout: Option<Duration>,
) -> Result<message::Ack, Error> {
    let req = OverrideRegistry::query::<Request, Commander>().await.send(Request(message));

    let resp = match timeout {
        Some(dur) => req.timeout(dur).await,
        None => req.await,
    };

    let resp: message::Ack = resp?.ok_or_else(|| Error::RequestDropped)?;
    if !resp.as_ref().header.ty.valid || !resp.as_ref().payload.as_ref().header.ty.valid {
        return Err(Error::InvalidChecksum);
    }

    Ok(resp)
}

#[inline]
pub async fn do_send(message: message::Message) {
    OverrideRegistry::query::<Request, Commander>().await.do_send(Request(message))
}

impl Commander {
    fn collect_garbage(&mut self) {
        let keys = self
            .requests
            .iter()
            .filter(|(_, v)| v.is_closed())
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();

        keys.into_iter().for_each(|k| {
            self.requests.remove(&k);
        });
    }
}

impl Actor for Commander {
    type Context = Context<Self>;

    #[tracing::instrument(skip_all)]
    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_async::<SystemBroker, serial::AckMessage>(ctx);

        ctx.run_interval(Duration::from_secs(5), |a, _ctx| {
            a.collect_garbage();
        });
    }
}

impl Supervised for Commander {}
impl SystemService for Commander {}

impl Handler<Request> for Commander {
    type Result = ResponseFuture<Option<message::Ack>>;

    fn handle(&mut self, msg: Request, ctx: &mut Self::Context) -> Self::Result {
        let (tx, rx) = oneshot::channel();
        self.requests.insert(msg.0.as_ref().header.unique_id(), tx);
        self.issue_sync::<SystemBroker, _>(serial::UpMessage(msg.0), ctx);

        Box::pin(async move { rx.await.ok() })
    }
}

impl Handler<serial::AckMessage> for Commander {
    type Result = ();

    fn handle(&mut self, msg: serial::AckMessage, _ctx: &mut Self::Context) -> Self::Result {
        let msg_id = msg.0.as_ref().payload.as_ref().header.unique_id();

        match self.requests.remove(&msg_id) {
            Some(tx) => {
                if let Err(_msg) = tx.send(msg.0) {
                    tracing::warn!(?msg_id, "tried to ack command, listener dropped");
                }
            },
            None => {
                tracing::warn!(?msg_id, "received ack message for unregistered command");
            },
        }
    }
}
