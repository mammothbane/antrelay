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

        ctx.spawn(
            fut::wrap_stream(sigstream)
                .map(|_x, a, ctx| {
                    a.issue_async(crate::signals::Term, ctx);
                })
                .finish(),
        );
    }
}

impl Supervised for UnixSignal {}

impl SystemService for UnixSignal {}
