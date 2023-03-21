use std::time::Duration;

use actix::{
    Actor,
    AsyncContext,
    Context,
    Handler,
    MailboxError,
    Message,
    ResponseFuture,
    Supervised,
    SystemService,
};
use actix_broker::{
    BrokerIssue,
    BrokerSubscribe,
    SystemBroker,
};
use futures::future::BoxFuture;
use tokio::sync::oneshot;

use message::{
    SourceInfo,
    UniqueId,
};

use crate::{
    serial,
    serial::AntMessage,
    OverrideRegistry,
};

#[derive(Debug, Clone, PartialEq, Message)]
#[rtype(result = "()")]
pub enum Response {
    Message(message::Message),
    Ant(message::AntPacket),
}

#[derive(Debug, Clone, PartialEq, Message)]
#[rtype(result = "Option<Response>")]
pub struct Request(pub message::Message);

pub struct Commander {
    requests: fnv::FnvHashMap<UniqueId, oneshot::Sender<Response>>,
    once:     std::sync::Once,
}

impl Default for Commander {
    fn default() -> Self {
        Self {
            requests: Default::default(),
            once:     std::sync::Once::new(),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Mailbox(#[from] MailboxError),

    #[error("request was dropped")]
    RequestDropped,

    #[error("original command checksum was invalid")]
    InvalidChecksum,

    #[error("response indicated command checksum was invalid")]
    InvalidAckChecksum,
}

#[tracing::instrument(skip(message, retry_strategy), err(Display))]
pub async fn send_retry(
    mut message: impl FnMut() -> BoxFuture<'static, message::Message>,
    request_timeout: Duration,
    retry_strategy: impl IntoIterator<Item = Duration>,
) -> Result<Response, Error> {
    tokio_retry::Retry::spawn(retry_strategy, || {
        let message = message();

        async move {
            let msg = message.await;
            let result = send(msg, Some(request_timeout)).await;

            if let Err(ref e) = result {
                tracing::warn!(error = %e, "message send failed");
            }

            result
        }
    })
    .await
}

#[inline]
#[tracing::instrument(fields(%message))]
pub async fn send(message: message::Message, timeout: Option<Duration>) -> Result<Response, Error> {
    let req = OverrideRegistry::query::<Request, Commander>().await.send(Request(message));

    let resp = match timeout {
        Some(dur) => req.timeout(dur).await,
        None => req.await,
    };

    tracing::debug!("awaiting reply");
    let resp = resp?.ok_or(Error::RequestDropped)?;

    let header = match &resp {
        Response::Ant(pkt) => pkt.header.header,
        Response::Message(msg) => msg.header.header,
    };

    if header.ty.invalid {
        return Err(Error::InvalidAckChecksum);
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
        self.once.call_once(|| {
            self.subscribe_async::<SystemBroker, serial::AckMessage>(ctx);
            self.subscribe_async::<SystemBroker, AntMessage>(ctx);
        });

        ctx.run_interval(Duration::from_secs(5), |a, _ctx| {
            a.collect_garbage();
        });
    }
}

impl Supervised for Commander {}
impl SystemService for Commander {}

impl Handler<Request> for Commander {
    type Result = ResponseFuture<Option<Response>>;

    fn handle(&mut self, msg: Request, _ctx: &mut Self::Context) -> Self::Result {
        let (tx, rx) = oneshot::channel();
        self.requests.insert(msg.0.as_ref().header.header.unique_id(), tx);
        self.issue_async::<SystemBroker, _>(serial::UpMessage(msg.0));

        Box::pin(async move { rx.await.ok() })
    }
}

impl Handler<serial::AckMessage> for Commander {
    type Result = ();

    fn handle(&mut self, msg: serial::AckMessage, _ctx: &mut Self::Context) -> Self::Result {
        // drop relay packets in favor of the inner ant message
        if matches!(msg.0.header.header.ty, message::header::MessageType {
            event: message::header::Event::CSRelay,
            disposition: message::header::Disposition::Ack,
            ..
        }) {
            return;
        }

        let SourceInfo::Info(info) = msg.0.header.payload else {
            return;
        };

        let msg_id = info.header.unique_id();

        let Some(tx) = self.requests.remove(&msg_id) else {
            tracing::warn!(?msg_id, "received ack message for unregistered command");
            return
        };

        if let Err(_msg) = tx.send(Response::Message(msg.0)) {
            tracing::warn!(?msg_id, "tried to ack command, listener dropped");
        }
    }
}

impl Handler<AntMessage> for Commander {
    type Result = ();

    fn handle(&mut self, msg: AntMessage, _ctx: &mut Self::Context) -> Self::Result {
        let SourceInfo::Info(info) = msg.0.header.payload else {
            return;
        };

        let msg_id = info.header.unique_id();

        let Some(tx) = self.requests.remove(&info.header.unique_id()) else {
            return;
        };

        if let Err(_e) = tx.send(Response::Ant(msg.0)) {
            tracing::warn!(?msg_id, "got ant ack for valid command whose listener was dropped");
        }
    }
}
