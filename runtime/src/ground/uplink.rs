use futures::future::BoxFuture;

type StaticSender<E> = dyn net::DatagramReceiver<Error = E> + 'static;

pub struct Uplink<E> {
    pub make: Box<dyn Fn() -> BoxFuture<'static, StaticSender<E>>>,
}
