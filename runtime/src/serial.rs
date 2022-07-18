use actix::{
    Actor,
    ActorContext,
    AsyncContext,
    Context,
    Handler,
    Message,
    Running,
    Supervised,
};
use actix_broker::{
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
};
use tokio::{
    io::{
        AsyncRead,
        AsyncWrite,
    },
    sync::mpsc,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Message)]
#[rtype(result = "()")]
pub struct Uplink(pub Bytes);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Message)]
#[rtype(result = "()")]
pub struct Downlink(pub Bytes);

type IO = (Box<dyn AsyncRead>, Box<dyn AsyncWrite>);

pub struct SerialIO {
    make_io: Box<dyn Fn() -> BoxFuture<Option<IO>>>,
    tx:      Option<mpsc::UnboundedSender<Bytes>>,
}

impl SerialIO {
    pub fn new(make_io: Box<dyn Fn() -> BoxFuture<Option<IO>>>) -> Self {
        Self {
            make_io,
            tx: None,
        }
    }
}

impl Actor for SerialIO {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let (r, w) = match ctx.wait(self.make_io()) {
            Some(io) => io,
            None => {
                ctx.stop();
                return;
            },
        };

        let (tx, rx) = mpsc::unbounded_channel();

        let framed_write = FramedWrite::new(w, CobsCodec);
        ctx.spawn(async move {
            let mut rx = rx.map(Ok);
            let mut framed_write = framed_write;

            framed_write.send_all(&mut rx).await.unwrap();
        });

        self.tx = Some(tx);
        self.subscribe_async::<SystemBroker, Uplink>(ctx);

        let framed_read = FramedRead::new(r, CobsCodec);
        ctx.add_message_stream(framed_read.map(Downlink));
    }
}

impl Handler<Uplink> for SerialIO {
    type Result = ();

    fn handle(&mut self, msg: Uplink, _ctx: &mut Self::Context) -> Self::Result {
        match self.tx {
            Some(ref tx) => {
                if let Err(_e) = tx.send(msg.0) {
                    tracing::error!("remote serial channel dropped");
                }
            },
            None => {
                unreachable!();
            },
        }
    }
}

impl Supervised for SerialIO
where
    Self: Actor,
{
    fn restarting(&mut self, _ctx: &mut <Self as Actor>::Context) {
        self.write = None;
    }
}
