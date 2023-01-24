use std::io;

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

type StaticReceiver = dyn net::DatagramReceiver + 'static + Unpin + Send + Sync;

pub struct Uplink {
    pub make_socket: Box<dyn Fn() -> BoxFuture<'static, Option<Box<StaticReceiver>>>>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct PacketResult(io::Result<ground::UpPacket>);

impl Actor for Uplink {
    type Context = Context<Self>;

    #[tracing::instrument(skip_all)]
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

                        Ok(Some((ground::UpPacket(ret), (recv, buf)))) as io::Result<_>
                    })
                    .map(PacketResult);

                ctx.add_message_stream(packets);
            });

        ctx.wait(f);
    }
}

impl Supervised for Uplink where Self: Actor {}

impl Handler<PacketResult> for Uplink
where
    Self: Actor,
    Self::Context: AsyncContext<Self>,
{
    type Result = ();

    fn handle(&mut self, item: PacketResult, ctx: &mut Self::Context) {
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
