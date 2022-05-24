use std::{
    future::Future,
    path::Path,
    thread,
};

use smol::net::unix::UnixDatagram;

pub fn signals() -> eyre::Result<smol::channel::Receiver<!>> {
    let (tx, rx) = smol::channel::unbounded();

    thread::spawn(move || {
        use signal_hook::{
            consts::*,
            iterator::Signals,
        };

        let mut signals = Signals::new(&[SIGTERM, SIGINT]).expect("registering signal handlers");

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

pub fn uds_connect(p: impl AsRef<Path>) -> eyre::Result<UnixDatagram> {
    let sock = UnixDatagram::unbound()?;
    sock.connect(p.as_ref())?;

    Ok(sock)
}
