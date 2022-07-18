use actix::{
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
        ctx.spawn(async move {
            tokio::signal::ctrl_c().await.expect("listening for ctrl-c");
            Broker::<SystemBroker>::issue_async(crate::signals::Term);
        });
    }
}

impl Supervised for WindowsSignal {}

impl SystemService for WindowsSignal {}
