use std::{
    cell::{
        RefCell,
        RefMut,
    },
    error::Error,
    fmt::Display,
    sync::Arc,
    time::Duration,
};

use backoff::backoff::Backoff;
use smol::stream::{
    Stream,
    StreamExt,
};
use tap::Pipe;
use tracing::Span;

use crate::stream_unwrap;

pub use datagram::{
    Datagram,
    DatagramReceiver,
    DatagramSender,
};

mod datagram;

#[cfg(unix)]
mod unix;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SocketMode {
    Connect,
    Bind,
}

pub fn socket_stream<Socket>(
    address: Socket::Address,
    backoff: impl Backoff + Clone + Send + Sync,
    mode: SocketMode,
) -> impl Stream<Item = Result<Socket, Socket::Error>> + Unpin + Send
where
    Socket: Datagram,
    Socket::Address: Clone + Send + Sync,
    Socket::Error: Display,
{
    smol::stream::unfold(backoff, move |backoff| {
        let address = address.clone();

        Box::pin(async move {
            let result = backoff::future::retry_notify(
                backoff.clone(),
                || async {
                    match mode {
                        SocketMode::Connect => Socket::connect(&address).await,
                        SocketMode::Bind => Socket::bind(&address).await,
                    }
                    .map_err(backoff::Error::transient)
                },
                |e, dur| {
                    tracing::error!(error = %e, backoff_dur = ?dur, ?mode, "connecting to socket");
                },
            )
            .await;

            Some((result, backoff))
        })
    })
}

#[tracing::instrument(skip_all, level = "debug")]
pub fn receive_packets<Socket>(
    sockets: impl Stream<Item = Socket> + Unpin,
) -> impl Stream<Item = Vec<u8>> + Unpin
where
    Socket: DatagramReceiver,
    Socket::Error: Error + Send + Sync + 'static,
{
    let span = Span::current();

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
        .pipe(stream_unwrap!(parent: &span, "reading from socket"))
}

#[tracing::instrument(skip_all, level = "debug")]
pub fn send_packets<Socket>(
    sockets: impl Stream<Item = Socket> + Unpin,
    packets: impl Stream<Item = Vec<u8>>,
) -> impl Stream<Item = Result<(), Socket::Error>>
where
    Socket: DatagramSender,
    Socket::Error: Error,
{
    tracing::debug!("waow starting");

    futures::StreamExt::scan(
        packets,
        (Arc::new(RefCell::new(sockets)), Arc::new(RefCell::new(None))),
        |(sockets, sock), pkt| {
            let sock = sock.clone();
            let sockets = sockets.clone();

            async move {
                let sock = sock;
                let sockets = sockets;

                let mut sock: RefMut<Option<Arc<Socket>>> = sock.borrow_mut();
                let mut sockets = sockets.borrow_mut();

                if sock.is_none() {
                    *sock = sockets.next().await.map(Arc::new);
                }

                let result = sock.as_mut()?.send(&pkt).await.map(|_| ());

                Some(result)
            }
        },
    )
}

lazy_static::lazy_static! {
    pub static ref DEFAULT_BACKOFF: backoff::exponential::ExponentialBackoff<backoff::SystemClock> = backoff::ExponentialBackoffBuilder::new()
        .with_initial_interval(Duration::from_millis(25))
        .with_max_interval(Duration::from_secs(3))
        .with_randomization_factor(0.5)
        .with_max_elapsed_time(None)
        .build();
}
