use std::{
    future::Future,
    path::Path,
    thread,
};

use smol::net::unix::UnixDatagram;

pub fn signals() -> eyre::Result<smol::channel::Receiver<!>> {
    use signal_hook::{
        consts::*,
        iterator::Signals,
    };

    let (tx, rx) = smol::channel::unbounded();
    let mut signals = Signals::new(&[SIGTERM, SIGINT])?;

    thread::spawn(move || {
        signals.wait().next();

        tx.close();
    });

    Ok(rx)
}

#[inline]
pub async fn either<T, U>(
    t: impl Future<Output = T>,
    u: impl Future<Output = U>,
) -> either::Either<T, U> {
    smol::future::or(async move { either::Left(t.await) }, async move { either::Right(u.await) })
        .await
}

#[tracing::instrument(fields(path = ?p.as_ref()), skip(p), err(Display))]
pub fn uds_connect(p: impl AsRef<Path>) -> eyre::Result<UnixDatagram> {
    let sock = UnixDatagram::unbound()?;
    sock.connect(p.as_ref())?;

    Ok(sock)
}

#[tracing::instrument(fields(path = ?p.as_ref()), skip(p), err(Display))]
pub async fn remove_and_bind(p: impl AsRef<Path>) -> eyre::Result<UnixDatagram> {
    match smol::fs::remove_file(&p).await {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
        result => result?,
    };

    let result = UnixDatagram::bind(&p)?;

    Ok(result)
}

#[macro_export]
macro_rules! bootstrap {
    ($x:expr $( , $xs:expr )* $(,)?) => {
        eprintln!(concat!("[bootstrap] ", $x) $( , $xs )*)
    };
}

pub use bootstrap;
