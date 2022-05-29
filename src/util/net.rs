use std::{
    cell::RefCell,
    error::Error,
    io,
    net::{
        Shutdown,
        SocketAddr,
    },
    rc::Rc,
    time::Duration,
};

use backoff::backoff::Backoff;
use smol::{
    net::UdpSocket,
    stream::{
        Stream,
        StreamExt,
    },
};

#[async_trait::async_trait]
pub trait DatagramSender: Sized {
    type Address;
    type Error = io::Error;

    async fn new(address: &Self::Address) -> Result<Self, Self::Error>;
    async fn send(&self, packet: &[u8]) -> Result<usize, Self::Error>;
    fn shutdown(&self, how: Shutdown) -> Result<(), Self::Error>;
    fn display_addr(addr: &Self::Address) -> String;
}

#[async_trait::async_trait]
impl DatagramSender for UdpSocket {
    type Address = SocketAddr;

    #[inline]
    async fn new(address: &SocketAddr) -> io::Result<Self> {
        let sock = UdpSocket::bind("localhost:0").await?;
        sock.connect(address).await?;

        Ok(sock)
    }

    #[inline]
    async fn send(&self, packet: &[u8]) -> io::Result<usize> {
        self.send(&packet).await
    }

    #[inline]
    fn shutdown(&self, _how: Shutdown) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn display_addr(addr: &SocketAddr) -> String {
        addr.to_string()
    }
}

#[tracing::instrument(skip(packets, backoff), fields(address = Socket::display_addr(&address).as_str()))]
async fn send_packets<Socket>(
    address: Socket::Address,
    packets: impl Stream<Item = Vec<u8>> + StreamExt + Unpin,
    backoff: impl Backoff + Clone,
) where
    Socket: DatagramSender,
    Socket::Error: Error,
{
    let sock: Rc<RefCell<Option<Socket>>> = Rc::new(RefCell::new(None));

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
        let addr = &address;
        let backoff = backoff.clone();

        let sock_result = backoff::future::retry_notify(
            backoff,
            || async { Socket::new(addr).await.map_err(backoff::Error::transient) },
            |e, dur| tracing::error!(error = %e, backoff_duration = ?dur, "connecting to socket"),
        )
        .await;

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
    static ref DEFAULT_BACKOFF: backoff::exponential::ExponentialBackoff<backoff::SystemClock> = backoff::ExponentialBackoffBuilder::new()
        .with_initial_interval(Duration::from_millis(25))
        .with_max_interval(Duration::from_secs(3))
        .with_randomization_factor(0.5)
        .with_max_elapsed_time(None)
        .build();
}

pub async fn tee_packets<Socket>(
    addrs: impl IntoIterator<Item = Socket::Address>,
    backoff: impl Backoff + Clone,
    packet_stream: async_broadcast::Receiver<Vec<u8>>,
) where
    Socket: DatagramSender,
    Socket::Error: Error,
{
    let futs = addrs
        .into_iter()
        .map(|addr| send_packets::<Socket>(addr, packet_stream.clone(), backoff.clone()));

    futures::future::join_all(futs).await;
}
