use std::{
    net::Shutdown,
    time::Duration,
};

use futures::AsyncWrite;
use smol::{
    io::AsyncWriteExt,
    net::unix::UnixDatagram,
};

use crate::util;

pub async fn uplink(
    uplink_socket: UnixDatagram,
    mut serial_write: impl AsyncWrite + Unpin,
    done: smol::channel::Receiver<!>,
) {
    let mut buf = vec![0; 4096];

    loop {
        let result: anyhow::Result<_> = try {
            let count = match util::either(uplink_socket.recv(&mut buf), done.recv()).await {
                either::Left(ret) => ret?,
                either::Right(_) => {
                    if let Err(e) = uplink_socket.shutdown(Shutdown::Both) {
                        tracing::error!(error = ?e, "shutting down uplink socket after done signal");
                    }

                    return;
                },
            };

            serial_write.write_all(&buf[..count]).await?;
        };

        if let Err(e) = result {
            tracing::error!(error = ?e, "uplink error, sleeping before retry");
            smol::Timer::after(Duration::from_millis(20)).await;
        }
    }
}
