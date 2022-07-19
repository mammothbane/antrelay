use actix::{
    fut::ActorFutureExt,
    prelude::*,
};
use actix_broker::{
    BrokerSubscribe,
    SystemBroker,
};
use futures::{
    future::BoxFuture,
    FutureExt,
    StreamExt,
};
use std::{
    fmt::Display,
    sync::Arc,
};

use crate::{
    ground,
    serial,
};
use net::DatagramSender;

type StaticSender<E> = dyn DatagramSender<Error = E> + 'static + Unpin;
type StaticSenders<E> = Vec<Arc<StaticSender<E>>>;

pub struct Downlink<E> {
    make:    Box<dyn Fn() -> BoxFuture<'static, Option<StaticSenders<E>>>>,
    senders: StaticSenders<E>,
}

impl<E> Downlink<E> {
    pub fn new(
        make_sockets: Box<dyn Fn() -> BoxFuture<'static, Option<StaticSenders<E>>>>,
    ) -> Self {
        Self {
            make:    make_sockets,
            senders: vec![],
        }
    }
}

impl<E> Actor for Downlink<E>
where
    E: Display + 'static,
{
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let run = fut::wrap_future::<_, Self>((self.make)()).map(|result, a, ctx| {
            match result {
                Some(vs) => {
                    a.senders = vs;
                },
                None => {
                    tracing::error!("failed to construct downlink");
                    ctx.stop();
                    return;
                },
            };

            a.subscribe_async::<SystemBroker, serial::raw::DownPacket>(ctx);
            // a.subscribe_async::<SystemBroker, serial::raw::UpPacket>(ctx);
            // a.subscribe_async::<SystemBroker, serial::serde::DownPacket>(ctx);
            // a.subscribe_async::<SystemBroker, serial::serde::UpPacket>(ctx);
            // a.subscribe_async::<SystemBroker, ground::UpPacket>(ctx);

            // TODO: log packets
        });

        ctx.wait(run);
    }
}

macro_rules! imp {
    ($msg:ty, $extract:expr) => {
        impl<E> Handler<$msg> for Downlink<E>
        where
            E: Display + 'static,
        {
            type Result = ();

            fn handle(&mut self, msg: $msg, ctx: &mut Self::Context) -> Self::Result {
                let b: ::bytes::Bytes = ($extract)(msg);

                let f = futures::stream::iter(self.senders.iter()).for_each_concurrent(None, |s| {
                    let b = b.clone();

                    async move {
                        if let Err(e) = s.send(b.as_ref()).await {
                            tracing::error!(error = %e, "sending packet to downlink");
                        }
                    }
                });

                ctx.wait(fut::wrap_future(f));
            }
        }
    };
}

// imp!(serial::raw::DownPacket, |msg: serial::raw::DownPacket| msg.0);

impl<E> Handler<serial::raw::DownPacket> for Downlink<E>
where
    E: Display + 'static,
{
    type Result = ();

    fn handle(&mut self, msg: serial::raw::DownPacket, ctx: &mut Self::Context) -> Self::Result {
        let f = futures::stream::iter(self.senders.clone()).for_each_concurrent(None, move |s| {
            let b = msg.0.clone();

            async move {
                if let Err(e) = s.send(b.as_ref()).await {
                    tracing::error!(error = %e, "sending packet to downlink");
                }
            }
        });

        ctx.wait(fut::wrap_future(f));
    }
}