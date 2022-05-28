use std::future::Future;

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::{
    dynload,
    remove_and_bind,
    signals,
    uds_connect,
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

pub use bootstrap;
