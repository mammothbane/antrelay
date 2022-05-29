use std::{
    net::Shutdown,
    path::Path,
    time::Duration,
};

use smol::{
    net::unix::UnixDatagram,
    stream::{
        Stream,
        StreamExt,
    },
};

#[async_trait::async_trait]
pub trait DatagramSocket {
    type Address;
    type Error = std::io::Error;

    fn new(address: Self::Address) -> Result<Self, Self::Error>;
    async fn send(&self, packet: &[u8]) -> Result<usize, Self::Error>;
    fn shutdown(&self, how: Shutdown) -> Result<(), Self::Error>;
}

pub async fn multisocket(
    paths: impl IntoIterator<Item = &Path>,
    packet_stream: impl Stream,
) -> eyre::Result<()> {
    paths.into_iter().map(|path| single_socket(path));
    Ok(())
}

#[tracing::instrument(skip(packets), fields(path = %path.display()))]
async fn single_socket(path: &Path, packets: impl Stream<Item = Vec<u8>> + StreamExt + Unpin) {
    let bk = backoff::ExponentialBackoffBuilder::new()
        .with_initial_interval(Duration::from_millis(25))
        .with_max_interval(Duration::from_secs(3))
        .with_randomization_factor(0.5)
        .with_max_elapsed_time(None)
        .build();

    loop {
        let sock = smol::unblock(|| {
            backoff::retry_notify(
                &bk,
                || crate::util::unix::uds_connect(path).map_err(backoff::Error::transient),
                |e, dur| tracing::error!(error = %e, backoff_duration = %dur, "connecting to unix socket"),
            )
        }).await;

        let sock = match sock {
            Err(backoff::Error::Permanent(e)) => return Err(e),
            Err(backoff::Error::Transient {
                err: _err,
                ..
            }) => unreachable!(),
            Ok(sock) => sock,
        };

        let send_result =
            packets.then(|pkt| sock.send(&pkt)).try_for_each(std::convert::identity).await;

        crate::trace_catch!(send_result, "socket send failed");
        crate::trace_catch!(sock.shutdown(Shutdown::Both), "shutting down socket");

        // the source stream ended
        if send_result.is_ok() {
            break;
        }
    }
}
