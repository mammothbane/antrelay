use std::{
    net::Shutdown,
    time::Duration,
};

use futures::AsyncWrite;
use smol::{
    io::AsyncWriteExt,
    net::unix::UnixDatagram,
};

use lunarrelay::util;

#[derive(Copy, Clone, Debug)]
enum UplinkStatus {
    Continue,
    Close,
}

#[tracing::instrument(skip_all)]
pub async fn uplink(
    uplink_socket: UnixDatagram,
    mut serial_write: impl AsyncWrite + Unpin,
    done: smol::channel::Receiver<!>,
) {
    let mut buf = vec![0; 1024];
    let mut serial_write = smol::io::BufWriter::new(&mut serial_write);

    loop {
        match uplink_once(&uplink_socket, &mut serial_write, &done, &mut buf[..]).await {
            Ok(UplinkStatus::Close) => break,
            Ok(UplinkStatus::Continue) => {},
            Err(e) => {
                tracing::error!(error = ?e, "uplink error, sleeping before retry");
                smol::Timer::after(Duration::from_millis(20)).await;
            },
        }
    }
}

#[tracing::instrument(skip_all)]
async fn uplink_once(
    sock: &UnixDatagram,
    serial_write: &mut (impl AsyncWrite + Unpin),
    done: &smol::channel::Receiver<!>,
    buf: &mut [u8],
) -> eyre::Result<UplinkStatus> {
    let count = match util::either(sock.recv(buf), done.recv()).await {
        either::Left(ret) => ret?,
        either::Right(_) => {
            if let Err(e) = sock.shutdown(Shutdown::Both) {
                tracing::error!(error = ?e, "shutting down uplink socket after done signal");
            }

            return Ok(UplinkStatus::Close);
        },
    };

    let cobs_vec = postcard::to_allocvec_cobs(&buf[..count])?;
    serial_write.write_all(&cobs_vec).await?;

    Ok(UplinkStatus::Continue)
}
