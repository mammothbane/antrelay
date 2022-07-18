use smol::net::UdpSocket;
use std::{
    io,
    net::{
        Shutdown,
        SocketAddr,
    },
};

#[async_trait::async_trait]
pub trait DatagramOps: Sized {
    type Address;
    type Error = io::Error;

    async fn connect(address: &Self::Address) -> Result<Self, Self::Error>;
    async fn bind(address: &Self::Address) -> Result<Self, Self::Error>;
    fn shutdown(&self, how: Shutdown) -> Result<(), Self::Error>;
    fn display_addr(addr: &Self::Address) -> String;
}

#[async_trait::async_trait]
pub trait DatagramReceiver {
    type Error = io::Error;
    async fn recv(&self, packet: &mut [u8]) -> Result<usize, Self::Error>;
}

#[async_trait::async_trait]
pub trait DatagramSender {
    type Error = io::Error;
    async fn send(&self, packet: &[u8]) -> Result<usize, Self::Error>;
}

#[async_trait::async_trait]
impl DatagramOps for UdpSocket {
    type Address = SocketAddr;

    #[tracing::instrument(err, fields(address = Self::display_addr(address).as_str()), level = "debug")]
    #[inline]
    async fn connect(address: &SocketAddr) -> io::Result<Self> {
        let sock = UdpSocket::bind("127.0.0.1:0").await?;
        sock.connect(address).await?;

        Ok(sock)
    }

    #[tracing::instrument(err, fields(address = Self::display_addr(address).as_str()), level = "debug")]
    #[inline]
    async fn bind(address: &Self::Address) -> Result<Self, Self::Error> {
        UdpSocket::bind(address).await
    }

    #[tracing::instrument(err, skip_all, level = "debug")]
    #[inline]
    fn shutdown(&self, _how: Shutdown) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn display_addr(addr: &SocketAddr) -> String {
        addr.to_string()
    }
}

#[async_trait::async_trait]
impl DatagramSender for UdpSocket {
    #[tracing::instrument(err, ret, fields(packet.len = packet.len(), self.addr = ?self.local_addr().ok()), skip(packet, self), level = "trace")]
    #[inline]
    async fn send(&self, packet: &[u8]) -> io::Result<usize> {
        self.send(&packet).await
    }
}

#[async_trait::async_trait]
impl DatagramReceiver for UdpSocket {
    #[tracing::instrument(err, ret, fields(buf.len = buf.len(), self.addr = ?self.local_addr().ok()), skip(self, buf), level = "trace")]
    #[inline]
    async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.recv(buf).await
    }
}

#[async_trait::async_trait]
impl DatagramSender for smol::channel::Sender<Vec<u8>> {
    type Error = smol::channel::SendError<Vec<u8>>;

    async fn send(&self, packet: &[u8]) -> Result<usize, Self::Error> {
        self.send(packet.to_vec()).await?;
        Ok(packet.len())
    }
}

#[async_trait::async_trait]
impl DatagramReceiver for smol::channel::Receiver<Vec<u8>> {
    type Error = smol::channel::RecvError;

    async fn recv(&self, packet: &mut [u8]) -> Result<usize, Self::Error> {
        let result = self.recv().await?;
        packet.copy_from_slice(&result[..]);

        Ok(result.len())
    }
}
