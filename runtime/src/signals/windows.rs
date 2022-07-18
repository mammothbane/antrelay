use actix::{
    fut,
    Actor,
    AsyncContext,
    Context,
    Supervised,
    SystemService,
};

use actix_broker::{
    Broker,
    BrokerIssue,
    SystemBroker,
};

#[derive(Default)]
pub struct WindowsSignal;

impl Actor for WindowsSignal {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.spawn(
            fut::wrap_future(async move {
                tokio::signal::ctrl_c().await.expect("listening for ctrl-c");
            })
            .map(|_, a, ctx| {
                a.issue_sync::<SystemBroker>(crate::signals::Term, ctx);
            }),
        );
    }
}

impl Supervised for WindowsSignal {}

impl SystemService for WindowsSignal {}
