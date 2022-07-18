use actix::{
    prelude::*,
    Actor,
    Addr,
    ArbiterHandle,
    AsyncContext,
    Context,
    Running,
    Supervised,
};
use actix_broker::{
    ArbiterBroker,
    Broker,
    BrokerIssue,
};
use signal_hook::consts::*;
use tokio::signal::unix::{
    signal,
    SignalKind,
};
use tokio_stream::StreamExt;

#[derive(Default)]
pub struct UnixSignal(Option<tokio::task::JoinHandle<()>>);

impl Actor for UnixSignal {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        let sigstream = try {
            let ints = signal(SignalKind::interrupt())?.map(|_| ());
            let terms = signal(SignalKind::interrupt())?.map(|_| ());

            ints.merge(terms)
        }
        .unwrap();

        ctx.spawn(async move {
            sigstream
                .for_each(|_| async {
                    Broker::<ArbiterBroker>::issue_async(crate::signals::Term);
                })
                .await;
        })
        .expect("spawning signal handler");
    }
}

impl Supervised for UnixSignal {}

impl SystemService for UnixSignal {}
