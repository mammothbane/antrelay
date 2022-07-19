use actix::prelude::*;
use actix_broker::{
    BrokerIssue,
    SystemBroker,
};
use bytes::BytesMut;
use futures::{
    future::BoxFuture,
    prelude::*,
};
use std::fmt::Display;

use crate::{
    context::ContextExt,
    ground,
};

type StaticReceiver<E> = dyn net::DatagramReceiver<Error = E> + 'static + Unpin;

pub struct Uplink<E> {
    pub make_socket: Box<dyn Fn() -> BoxFuture<'static, Option<Box<StaticReceiver<E>>>>>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct PacketResult<E>(Result<ground::UpPacket, E>);

impl<E> Actor for Uplink<E>
where
    E: Display + 'static,
{
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let receiver = match ctx.await_((self.make_socket)()) {
            Some(x) => x,
            None => {
                tracing::error!("failed to construct uplink socket");
                ctx.stop();
                return;
            },
        };

        let packets = futures::stream::try_unfold(
            (receiver, BytesMut::new()),
            |(recv, mut buf)| async move {
                buf.reserve(8192);

                let count: usize = recv.recv(buf.as_mut()).await?;
                let ret = buf.split_to(count);

                Ok(Some((ground::UpPacket(ret.freeze()), (recv, buf))))
            },
        )
        .map(PacketResult);

        ctx.add_message_stream(packets);
    }
}

impl<E> Handler<PacketResult<E>> for Uplink<E>
where
    Self: Actor,
    Self::Context: AsyncContext<Self>,
    E: Display + 'static,
{
    type Result = ();

    fn handle(&mut self, item: PacketResult<E>, ctx: &mut Self::Context) {
        let pkt = match item.0 {
            Ok(pkt) => pkt,
            Err(e) => {
                tracing::error!(error = %e, "handling packet");
                ctx.stop();
                return;
            },
        };

        self.issue_sync::<SystemBroker, _>(pkt, ctx);
    }
}
