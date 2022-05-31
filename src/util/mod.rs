use std::future::Future;

use smol::stream::{
    Stream,
    StreamExt,
};

#[cfg(unix)]
pub mod dynload;

#[inline]
pub async fn either<T, U>(
    t: impl Future<Output = T>,
    u: impl Future<Output = U>,
) -> either::Either<T, U> {
    smol::future::or(async move { either::Left(t.await) }, async move { either::Right(u.await) })
        .await
}

pub fn splittable_stream<S>(s: S, buffer: usize) -> impl Stream<Item = S::Item> + Clone + Unpin
where
    S: Stream + Send + 'static,
    S::Item: Clone + Send + Sync,
{
    let (tx, rx) = {
        let (mut tx, rx) = async_broadcast::broadcast(buffer);
        tx.set_overflow(true);

        (tx, rx.deactivate())
    };

    smol::spawn(async move {
        let tx = tx;

        s.for_each(|s| {
            trace_catch!(tx.try_broadcast(s), "broadcasting to splittable stream");
        })
        .await;
    })
    .detach();

    rx.activate_cloned()
}

#[inline]
pub fn log_and_discard_errors<T, E>(
    s: impl Stream<Item = Result<T, E>>,
    msg: &'static str,
) -> impl Stream<Item = T>
where
    E: std::fmt::Display,
{
    s.filter_map(move |x| {
        trace_catch!(x, msg);

        x.ok()
    })
}

#[macro_export]
macro_rules! bootstrap {
    ($x:expr $( , $xs:expr )* $(,)?) => {
        eprintln!(concat!("[bootstrap] ", $x) $( , $xs )*)
    };
}

#[macro_export]
macro_rules! trace_catch {
    ($val:expr, $($rest:tt)*) => {
        if let Err(ref e) = $val {
            ::tracing::error!(error = %e, $($rest)*);
        }
    };
}

pub use bootstrap;
pub use trace_catch;
