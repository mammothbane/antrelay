use std::future::Future;

mod tracing;
#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::{
    dynload,
    remove_and_bind,
    signals,
    uds_connect,
};

pub use self::tracing::{
    Value,
    Values,
};

#[inline]
pub async fn either<T, U>(
    t: impl Future<Output = T>,
    u: impl Future<Output = U>,
) -> either::Either<T, U> {
    smol::future::or(async move { either::Left(t.await) }, async move { either::Right(u.await) })
        .await
}

#[macro_export]
macro_rules! bootstrap {
    ($x:expr $( , $xs:expr )* $(,)?) => {
        eprintln!(concat!("[bootstrap] ", $x) $( , $xs )*)
    };
}

#[macro_export]
macro_rules! trace_catch {
    ($val:expr, $message:literal) => {
        if let Err(e) = $val {
            ::tracing::error!(error = %e, $message);
        }
    };
}

pub use bootstrap;
pub use trace_catch;
