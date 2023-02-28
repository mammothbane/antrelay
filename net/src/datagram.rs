use std::{
    io,
    net::{
        Shutdown,
        SocketAddr,
    },
};

use tokio::{
    net::UdpSocket,
    sync::mpsc,
};

#[async_trait::async_trait]
pub trait DatagramOps: Sized {
    type Address;

    async fn connect(address: &Self::Address) -> io::Result<Self>;
    async fn bind(address: &Self::Address) -> io::Result<Self>;
    fn shutdown(&self, how: Shutdown) -> io::Result<()>;
    fn display_addr(addr: &Self::Address) -> String;
}

#[async_trait::async_trait]
pub trait DatagramReceiver {
    async fn recv(&self, packet: &mut [u8]) -> io::Result<usize>;
}

#[async_trait::async_trait]
pub trait DatagramSender {
    async fn send(&self, packet: &[u8]) -> io::Result<usize>;
}

#[async_trait::async_trait]
impl DatagramOps for UdpSocket {
    type Address = SocketAddr;

    #[tracing::instrument(err, fields(address = Self::display_addr(address).as_str()))]
    #[inline]
    async fn connect(address: &SocketAddr) -> io::Result<Self> {
        let sock = UdpSocket::bind("127.0.0.1:0").await?;
        sock.connect(address).await?;

        Ok(sock)
    }

    #[tracing::instrument(err, fields(address = Self::display_addr(address).as_str()))]
    #[inline]
    async fn bind(address: &Self::Address) -> io::Result<Self> {
        UdpSocket::bind(address).await
    }

    #[tracing::instrument(err, skip_all)]
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
    #[tracing::instrument(err, fields(packet.len = packet.len(), self.addr = ?self.local_addr().ok()), skip(packet, self))]
    #[inline]
    async fn send(&self, packet: &[u8]) -> io::Result<usize> {
        self.send(packet).await
    }
}

#[async_trait::async_trait]
impl DatagramReceiver for UdpSocket {
    #[tracing::instrument(err, fields(buf.len = buf.len(), self.addr = ?self.local_addr().ok()), skip(self, buf))]
    #[inline]
    async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.recv(buf).await
    }
}

#[async_trait::async_trait]
impl DatagramSender for mpsc::Sender<Vec<u8>> {
    async fn send(&self, packet: &[u8]) -> io::Result<usize> {
        self.send(packet.to_vec())
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::ConnectionAborted, e))?;
        Ok(packet.len())
    }
}

#[async_trait::async_trait]
impl DatagramReceiver for tokio::sync::Mutex<mpsc::Receiver<Vec<u8>>> {
    async fn recv(&self, packet: &mut [u8]) -> io::Result<usize> {
        let result = {
            let mut lck = self.lock().await;
            lck.recv().await
        }
        .ok_or(io::Error::new(io::ErrorKind::ConnectionAborted, "remote end of channel closed"))?;

        packet.copy_from_slice(&result[..]);

        Ok(result.len())
    }
}
