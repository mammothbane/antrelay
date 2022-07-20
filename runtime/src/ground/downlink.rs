use std::{
    fmt::Display,
    sync::Arc,
};

use actix::{
    fut::ActorFutureExt,
    prelude::*,
};
use actix_broker::{
    BrokerSubscribe,
    SystemBroker,
};
use futures::future::BoxFuture;

use message::Downlink as DownlinkMsg;
use net::DatagramSender;

use crate::{
    ground,
    serial,
};

pub type StaticSender<E> = dyn DatagramSender<Error = E> + 'static + Unpin + Send + Sync;
pub type BoxSender<E> = Arc<StaticSender<E>>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Serialize(#[from] bincode::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

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

            // TODO(msg encoding)
            #[::tracing::instrument(skip_all, level = "trace")]
            fn handle(&mut self, msg: $msg, ctx: &mut Self::Context) -> Self::Result {
                let result: Result<Vec<u8>, Error> = try {
                    let d: ::message::Downlink = ($extract)(&msg);
                    let encoded = ::bincode::serialize(&d, ::bincode::Infinite)?;
                    let compressed = ::util::brotli_compress(&encoded)?;

                    compressed
                };

                let result = match result {
                    Ok(result) => result,
                    Err(e) => {
                        tracing::error!(error = %e, ty = stringify!($msg), "serializing downlink data");
                        return;
                    }
                };

                let sender = self.sender.as_ref().unwrap().clone();

                ctx.wait(fut::wrap_future(async move {
                    if let Err(e) = sender.send(&result).await {
                        tracing::error!(error = %e, "sending packet to downlink");
                    }
                }));
            }
        }
    };
}

imp!(serial::raw::DownPacket, |msg: &serial::raw::DownPacket| DownlinkMsg::SerialDownlinkRaw(
    msg.0.clone().into()
));
imp!(serial::raw::UpPacket, |msg: &serial::raw::UpPacket| DownlinkMsg::SerialUplinkRaw(
    msg.0.clone().into()
));
imp!(ground::UpPacket, |msg: &ground::UpPacket| DownlinkMsg::UplinkMirror(msg.0.clone().into()));

imp!(serial::UpMessage, |msg: &serial::UpMessage| { DownlinkMsg::SerialUplink(msg.0.clone()) });
imp!(serial::DownMessage, |msg: &serial::DownMessage| {
    DownlinkMsg::SerialDownlink(msg.0.clone())
});
