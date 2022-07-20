use std::time::Duration;

use actix::prelude::*;
use actix_broker::{
    BrokerSubscribe,
    SystemBroker,
};

#[derive(Clone, Debug, Message)]
#[rtype(result = "()")]
struct Message(usize);

struct Act;

impl Actor for Act {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("start");

        self.subscribe_async::<SystemBroker, Message>(ctx);

        ctx.wait(
            fut::wrap_future(async {
                tokio::time::sleep(Duration::from_millis(50)).await;
            })
            .map(|_, _, ctx: &mut Context<Self>| {
                println!("stop");
                ctx.stop();
            }),
        );
    }
}

impl Supervised for Act {
    fn restarting(&mut self, _ctx: &mut <Self as Actor>::Context) {
        println!("restarting");
    }
}

impl Handler<Message> for Act {
    type Result = AtomicResponse<Self, ()>;

    fn handle(&mut self, msg: Message, ctx: &mut Self::Context) -> Self::Result {
        println!("{}", msg.0);

        if msg.0 == 5 {
            ctx.stop();
        }

        AtomicResponse::new(Box::pin(
            async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                println!("{} slept", msg.0);
            }
            .into_actor(self),
        ))
    }
}

#[actix::test]
async fn mailbox_sanity() {
    let addr = Supervisor::start(|_| Act);

    (0..10).for_each(|x| {
        addr.do_send(Message(x));
    });

    tokio::time::sleep(Duration::from_millis(125)).await;

    panic!();
}
