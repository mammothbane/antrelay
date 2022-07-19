use actix::{
    prelude::*,
    Actor,
    AsyncContext,
};

use futures::Future;

pub trait ContextExt<A>: AsyncContext<A>
where
    A: Actor<Context = Self>,
{
    fn await_<F>(&mut self, f: F) -> F::Output
    where
        F: Future + 'static,
        F::Output: 'static;
}

impl<A, T> ContextExt<A> for T
where
    T: AsyncContext<A>,
    A: Actor<Context = T>,
{
    fn await_<F>(&mut self, f: F) -> F::Output
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let (tx, mut rx) = tokio::sync::oneshot::channel();

        self.wait(fut::wrap_future(async move {
            if let Err(_) = tx.send(f.await) {
                panic!("unable to resolve future");
            }
        }));

        rx.try_recv().unwrap()
    }
}
