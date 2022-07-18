use actix::{
    fut,
    fut::{
        ActorFutureExt,
        ActorStreamExt,
    },
    prelude::*,
    AsyncContext,
    Context,
    Handler,
    Message,
};
use actix_broker::{
    BrokerIssue,
    BrokerSubscribe,
    SystemBroker,
};
use antrelay_codec::{
    tokio_codec::{
        FramedRead,
        FramedWrite,
    },
    CobsCodec,
};
use bytes::Bytes;
use futures::{
    future::BoxFuture,
    SinkExt,
    StreamExt,
};
use tokio::{
    io::{
        AsyncRead,
        AsyncWrite,
    },
    sync::mpsc,
};
use tokio_stream::wrappers::UnboundedReceiverStream;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Message)]
#[rtype(result = "()")]
pub struct UpPacket(pub Bytes);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Message)]
#[rtype(result = "()")]
pub struct DownPacket(pub Bytes);

type IO = (Box<dyn AsyncRead + Unpin>, Box<dyn AsyncWrite + Unpin>);

pub struct RawIO {
    make_io: Box<dyn Fn() -> BoxFuture<'static, Option<IO>>>,
    tx:      Option<mpsc::UnboundedSender<Bytes>>,
}

impl RawIO {
    pub fn new(make_io: Box<dyn Fn() -> BoxFuture<'static, Option<IO>>>) -> Self {
        Self {
            make_io,
            tx: None,
        }
    }
}

impl Actor for RawIO {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.wait(fut::wrap_future((self.make_io)()).map(
            |result, a: &mut Self, ctx: &mut Context<Self>| {
                let (r, w) = match result {
                    Some(io) => io,
                    None => {
                        ctx.stop();
                        return;
                    },
                };

                let (tx, rx) = mpsc::unbounded_channel();
                let framed_write = FramedWrite::new(w, CobsCodec);

                ctx.spawn(fut::wrap_future(async move {
                    let mut rx = UnboundedReceiverStream::new(rx).map(Ok);
                    let mut framed_write = framed_write;

                    framed_write.send_all(&mut rx).await.unwrap();
                }));

                a.tx = Some(tx);
                a.subscribe_async::<SystemBroker, UpPacket>(ctx);

                let framed_downlink = FramedRead::new(r, CobsCodec);

                ctx.spawn(
                    fut::wrap_stream(framed_downlink.map(|x| x.map(DownPacket)))
                        .map(|x, a: &mut Self, ctx| {
                            let pkt = match x {
                                Ok(pkt) => pkt,
                                Err(e) => {
                                    tracing::error!(error = %e);
                                    return;
                                },
                            };

                            a.issue_sync::<SystemBroker, _>(pkt, ctx);
                        })
                        .finish(),
                );
            },
        ));
    }
}

impl Handler<UpPacket> for RawIO {
    type Result = ();

    fn handle(&mut self, msg: UpPacket, _ctx: &mut Self::Context) -> Self::Result {
        match self.tx {
            Some(ref tx) => {
                if let Err(_e) = tx.send(msg.0) {
                    tracing::error!("trying to send serial uplink packet: remote channel dropped");
                }
            },
            None => {
                unreachable!();
            },
        }
    }
}

impl Supervised for RawIO
where
    Self: Actor,
{
    fn restarting(&mut self, _ctx: &mut <Self as Actor>::Context) {
        self.tx = None;
    }
}
