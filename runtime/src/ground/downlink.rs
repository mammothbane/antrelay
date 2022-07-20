use actix::{
    fut::ActorFutureExt,
    prelude::*,
};
use actix_broker::{
    BrokerSubscribe,
    SystemBroker,
};
use futures::future::BoxFuture;
use packed_struct::PackedStructSlice;
use std::{
    fmt::Display,
    sync::Arc,
};

use crate::{
    ground,
    serial,
};
use net::DatagramSender;

pub type StaticSender<E> = dyn DatagramSender<Error = E> + 'static + Unpin + Send + Sync;
pub type BoxSender<E> = Arc<StaticSender<E>>;

pub struct Downlink<E> {
    make_socket: Box<dyn Fn() -> BoxFuture<'static, Option<BoxSender<E>>>>,
    sender:      Option<BoxSender<E>>,
}

impl<E> Downlink<E> {
    pub fn new(make_socket: Box<dyn Fn() -> BoxFuture<'static, Option<BoxSender<E>>>>) -> Self {
        Self {
            make_socket,
            sender: None,
        }
    }
}

impl<E> Actor for Downlink<E>
where
    E: Display + 'static,
{
    type Context = Context<Self>;

    #[tracing::instrument(level = "debug", skip_all)]
    fn started(&mut self, ctx: &mut Self::Context) {
        let run = fut::wrap_future::<_, Self>((self.make_socket)()).map(|result, a, ctx| {
            match result {
                Some(sender) => {
                    tracing::info!("connected to downlink socket");
                    a.sender = Some(sender);
                },
                None => {
                    tracing::error!("failed to construct downlink");
                    ctx.stop();
                    return;
                },
            };

            a.subscribe_async::<SystemBroker, serial::raw::DownPacket>(ctx);
            a.subscribe_async::<SystemBroker, serial::raw::UpPacket>(ctx);
            a.subscribe_async::<SystemBroker, serial::DownMessage>(ctx);
            a.subscribe_async::<SystemBroker, serial::UpMessage>(ctx);
            a.subscribe_async::<SystemBroker, ground::UpPacket>(ctx);

            // TODO: log packets
        });

        ctx.wait(run);
    }
}

impl<E> Supervised for Downlink<E> where Self: Actor {}

macro_rules! imp {
    ($msg:ty, $extract:expr) => {
        impl<E> Handler<$msg> for Downlink<E>
        where
            E: Display + 'static,
        {
            type Result = ();

            fn handle(&mut self, msg: $msg, ctx: &mut Self::Context) -> Self::Result {
                let b = ($extract)(&msg);

                let compressed = match ::util::brotli_compress(&b) {
                    Ok(compressed) => compressed,
                    Err(e) => {
                        tracing::error!(error = %e, "failed to compress downlink data");
                        return;
                    },
                };

                let sender = self.sender.as_ref().unwrap().clone();

                ctx.wait(fut::wrap_future(async move {
                    if let Err(e) = sender.send(&compressed).await {
                        tracing::error!(error = %e, "sending packet to downlink");
                    }
                }));
            }
        }
    };
}

imp!(serial::raw::DownPacket, |msg: &serial::raw::DownPacket| msg.0.clone());
imp!(serial::raw::UpPacket, |msg: &serial::raw::UpPacket| msg.0.clone());
imp!(ground::UpPacket, |msg: &ground::UpPacket| msg.0.clone());

imp!(serial::UpMessage, |msg: &serial::UpMessage| {
    match msg.0.pack_to_vec() {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "packing serial message");
            vec![]
        },
    }
});
imp!(serial::DownMessage, |msg: &serial::DownMessage| {
    match msg.0.pack_to_vec() {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "packing serial message");
            vec![]
        },
    }
});
