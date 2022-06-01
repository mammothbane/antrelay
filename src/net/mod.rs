use std::{
    error::Error,
    fmt::Display,
    net::Shutdown,
    sync::Arc,
    time::Duration,
};

use backoff::backoff::Backoff;
use smol::stream::{
    Stream,
    StreamExt,
};
use tap::Pipe;

use crate::util;

pub use self::datagram::{
    Datagram,
    DatagramReceiver,
    DatagramSender,
};

mod datagram;

#[cfg(unix)]
mod unix;

pub fn socket_connects<Socket>(
    address: Socket::Address,
    backoff: impl Backoff + Clone,
) -> impl Stream<Item = Result<Socket, Socket::Error>> + Unpin
where
    Socket: Datagram,
    Socket::Address: Clone,
    Socket::Error: Display,
{
    smol::stream::unfold(backoff, move |backoff| {
        let address = address.clone();

        Box::pin(async move {
            let result = backoff::future::retry_notify(
                backoff.clone(),
                || async { Socket::connect(&address).await.map_err(backoff::Error::transient) },
                |e, dur| {
                    tracing::error!(error = %e, backoff_dur = ?dur, "connecting to socket");
                },
            )
            .await;

            Some((result, backoff))
        })
    })
}

pub fn socket_binds<Socket>(
    address: Socket::Address,
    backoff: impl Backoff + Clone,
) -> impl Stream<Item = Result<Socket, Socket::Error>> + Unpin
where
    Socket: Datagram,
    Socket::Address: Clone + 'static,
    Socket::Error: Display,
{
    smol::stream::unfold(backoff, move |backoff| {
        let address = address.clone();

        Box::pin({
            async move {
                let result = backoff::future::retry_notify(
                    backoff.clone(),
                    || async { Socket::bind(&address).await.map_err(backoff::Error::transient) },
                    |e, dur| tracing::error!(error = %e, backoff_duration = ?dur, "connecting to socket"),
                )
                    .await;

                Some((result, backoff))
            }
        })
    })
}

#[tracing::instrument(skip_all)]
pub fn receive_packets<Socket>(
    sockets: impl Stream<Item = Socket>,
) -> impl Stream<Item = Vec<u8>> + Unpin
where
    Socket: DatagramReceiver,
    Socket::Error: Error + Send + Sync + 'static,
{
    sockets
        .flat_map(|sock| {
            Box::pin(smol::stream::try_unfold((sock, vec![0u8; 8192]), |(sock, mut buf)| {
                Box::pin(async move {
                    tracing::trace!("waiting for data from socket");

                    let count = sock.recv(&mut buf).await?;
                    Ok(Some((buf[..count].to_vec(), (sock, buf))))
                        as eyre::Result<Option<(Vec<u8>, _)>>
                })
            }))
        })
        .pipe(|s| util::log_and_discard_errors(s, "reading from socket"))
}

#[tracing::instrument(skip_all)]
pub async fn send_packets<Socket>(
    sockets: impl Stream<Item = Result<Socket, Socket::Error>>,
    packets: impl Stream<Item = Vec<u8>> + StreamExt + Unpin,
) where
    Socket: DatagramSender,
    Socket::Error: Error,
{
    sockets.flat_map(|socket| packets.then(|pkt| socket.send(&pkt))).try_for_each();

    let mut send_stream = packets.then(|pkt| {
        let sock = sock.clone();

        Box::pin(async move {
            let pkt = pkt;

            let sock = sock.borrow();
            let sock = sock.as_ref().unwrap();

            sock.send(&pkt).await
        })
    });

    loop {
        let sock_result = sockets.next().await?;

        match sock_result {
            Ok(s) => {
                {
                    let sock = sock.borrow();
                    if let Some(sock) = sock.as_ref() {
                        crate::trace_catch!(sock.shutdown(Shutdown::Both), "shutting down socket");
                    }
                }

                sock.replace(Some(s))
            },
            Err(_) => unreachable!(),
        };

        let send_result = send_stream.try_for_each(|result| result.map(|_| ())).await;

        crate::trace_catch!(send_result, "socket send failed");

        if send_result.is_ok() {
            tracing::info!("source stream closed, shutting down");
            break;
        }
    }

    let sock = sock.borrow();
    if let Some(sock) = sock.as_ref() {
        crate::trace_catch!(sock.shutdown(Shutdown::Both), "shutting down socket");
    }
}

lazy_static::lazy_static! {
    pub static ref DEFAULT_BACKOFF: backoff::exponential::ExponentialBackoff<backoff::SystemClock> = backoff::ExponentialBackoffBuilder::new()
        .with_initial_interval(Duration::from_millis(25))
        .with_max_interval(Duration::from_secs(3))
        .with_randomization_factor(0.5)
        .with_max_elapsed_time(None)
        .build();
}
