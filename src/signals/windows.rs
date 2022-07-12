use actix::{
    Actor,
    Context,
};

use actix_broker::{
    ArbiterBroker,
    Broker,
    BrokerIssue,
};

#[derive(Default)]
pub struct WindowsSignal;

impl Actor for WindowsSignal {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        ctrlc::set_handler(|| Broker::<ArbiterBroker>::issue_async(crate::signals::Term)).unwrap();
    }
}
