use std::{
    io,
    sync::Arc,
    time::Duration,
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

pub type StaticSender = dyn DatagramSender + 'static + Unpin + Send + Sync;
pub type BoxSender = Arc<StaticSender>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Serialize(#[from] bincode::Error),

    #[error(transparent)]
    Io(#[from] io::Error),
}

pub struct Downlink {
    make_socket:    Box<dyn Fn() -> BoxFuture<'static, Option<BoxSender>>>,
    sender:         Option<BoxSender>,
    subscribe_once: std::sync::Once,
}

impl Downlink {
    pub fn new(make_socket: Box<dyn Fn() -> BoxFuture<'static, Option<BoxSender>>>) -> Self {
        Self {
            make_socket,
            sender: None,
            subscribe_once: std::sync::Once::new(),
        }
    }
}

impl Actor for Downlink {
    type Context = Context<Self>;

    #[tracing::instrument(skip_all)]
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

            // pretty hateful
            a.subscribe_once.call_once(|| {
                a.subscribe_async::<SystemBroker, serial::raw::DownPacket>(ctx);
                a.subscribe_async::<SystemBroker, serial::raw::UpPacket>(ctx);
                a.subscribe_async::<SystemBroker, serial::DownMessage>(ctx);
                a.subscribe_async::<SystemBroker, serial::UpMessage>(ctx);
                a.subscribe_async::<SystemBroker, ground::UpPacket>(ctx);
                a.subscribe_async::<SystemBroker, ground::UpCommand>(ctx);
                a.subscribe_async::<SystemBroker, ground::Log>(ctx);
            });
        });

        ctx.wait(run);
    }
}

impl Supervised for Downlink
where
    Self: Actor,
{
    #[tracing::instrument(skip_all)]
    fn restarting(&mut self, ctx: &mut <Self as Actor>::Context) {
        tracing::warn!("restarting downlink");

        ctx.wait(fut::wrap_future(async move {
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }));
    }
}

#[tracing::instrument(skip_all)]
#[inline]
fn gen_handle<T>(
    extract: impl Fn(&T) -> message::Downlink + 'static,
    sender: Option<&BoxSender>,
    ctx: &mut Context<Downlink>,
    name: &'static str,
    msg: T,
) {
    let result: Result<Vec<u8>, Error> = try {
        let d: message::Downlink = extract(&msg);
        tracing::trace!(msg = %d, "downlinking");

        let encoded = bincode::serialize(&d, ::bincode::Infinite)?;
        let compressed = util::brotli_compress(&encoded)?;

        compressed
    };

    let result = match result {
        Ok(result) => result,
        Err(e) => {
            tracing::error!(error = %e, ty = %name, "serializing downlink data");
            return;
        },
    };

    let sender = sender.unwrap().clone();

    ctx.wait(
        fut::wrap_future(async move {
            let result = result;
            sender.send(&result).await
        })
        .map(|result, _a, ctx: &mut Context<Downlink>| {
            if let Err(e) = result {
                tracing::error!(error = %e, "sending packet to downlink");

                if e.kind() == io::ErrorKind::NotConnected {
                    ctx.stop();
                }
            }
        }),
    );
}

macro_rules! imp {
    ($msg:ty, $extract:expr) => {
        imp!($msg, $extract, |msg: &$msg| {
            let m = msg.as_ref();
            format!("{}", m)
        });
    };

    ($msg:ty, $extract:expr, $display:expr) => {
        impl Handler<$msg> for Downlink {
            type Result = ();

            #[allow(clippy::redundant_closure_call)]
            #[::tracing::instrument(skip_all, fields(msg = %($display)(&msg)))]
            fn handle(&mut self, msg: $msg, ctx: &mut Self::Context) -> Self::Result {
                gen_handle($extract, self.sender.as_ref(), ctx, stringify!($msg), msg);
            }
        }
    };
}

imp!(
    serial::raw::DownPacket,
    |msg: &serial::raw::DownPacket| DownlinkMsg::SerialDownlinkRaw(msg.0.clone().into()),
    |msg: &serial::raw::DownPacket| hex::encode(msg.0.as_ref())
);
imp!(
    serial::raw::UpPacket,
    |msg: &serial::raw::UpPacket| DownlinkMsg::SerialUplinkRaw(msg.0.clone().into()),
    |msg: &serial::raw::UpPacket| hex::encode(msg.0.as_ref())
);
imp!(
    ground::UpPacket,
    |msg: &ground::UpPacket| DownlinkMsg::UplinkMirror(msg.0.clone().into()),
    |msg: &ground::UpPacket| hex::encode(msg.0.as_ref())
);

imp!(serial::UpMessage, |msg: &serial::UpMessage| { DownlinkMsg::SerialUplink(msg.0.clone()) });
imp!(serial::DownMessage, |msg: &serial::DownMessage| {
    DownlinkMsg::SerialDownlink(msg.0.clone())
});
imp!(ground::UpCommand, |msg: &ground::UpCommand| DownlinkMsg::UplinkInterpreted(msg.0.clone()));
imp!(ground::Log, |msg: &ground::Log| DownlinkMsg::Log(msg.0.clone()));
