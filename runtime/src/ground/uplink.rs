use std::fmt::Display;

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
use message::BytesWrap;
use packed_struct::PackedStructSlice;

use crate::ground;

type StaticReceiver<E> = dyn net::DatagramReceiver<Error = E> + 'static + Unpin + Send + Sync;

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

    #[tracing::instrument(level = "debug", skip_all)]
    fn started(&mut self, ctx: &mut Self::Context) {
        let f =
            fut::wrap_future((self.make_socket)()).map(|result, _a, ctx: &mut Context<Self>| {
                let receiver = match result {
                    Some(x) => x,
                    None => {
                        tracing::error!("failed to construct uplink socket");
                        ctx.stop();
                        return;
                    },
                };

                tracing::info!("connected to uplink socket");

                let packets =
                    stream::try_unfold((receiver, BytesMut::new()), |(recv, mut buf)| async move {
                        buf.resize(8192, 0);

                        let count: usize = recv.recv(buf.as_mut()).await?;
                        let ret = buf.split_to(count).freeze();

                        Ok(Some((ground::UpPacket(ret), (recv, buf))))
                    })
                    .map(PacketResult);

                ctx.add_message_stream(packets);
            });

        ctx.wait(f);
    }
}

impl<E> Supervised for Uplink<E> where Self: Actor {}

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
                tracing::error!(error = %e, "receiving packet");
                ctx.stop();
                return;
            },
        };

        tracing::info!(hex = %hex::encode(&*pkt.0), "raw uplink command packet");
        self.issue_sync::<SystemBroker, _>(pkt.clone(), ctx);

        let msg = match <message::Message<BytesWrap> as PackedStructSlice>::unpack_from_slice(
            pkt.0.as_ref(),
        ) {
            Ok(msg) => msg,
            Err(e) => {
                tracing::error!(error = %e, "bad uplink message format");
                return;
            },
        };

        tracing::info!(%msg, "decoded uplink packet");
        self.issue_system_sync(ground::UpCommand(msg), ctx);
    }
}
