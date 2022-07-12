use actix::{
    Actor,
    Addr,
    ArbiterHandle,
    Context,
    Running,
};
use actix_broker::{
    ArbiterBroker,
    Broker,
    BrokerIssue,
};
use signal_hook::consts::*;

#[derive(Default)]
pub struct UnixSignal(Option<tokio::task::JoinHandle<()>>);

impl Actor for UnixSignal {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        self.0 = Some(actix::spawn(async move {
            let mut signals = signal_hook_tokio::Signals::new(&[SIGTERM, SIGINT]).unwrap();

            signals.next().await;

            Broker::<ArbiterBroker>::issue_async(crate::signals::Term);
        }));
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        self.0.and_then(|handle| handle.abort());
    }
}
