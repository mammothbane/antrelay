use smol::net::UdpSocket;
use std::{
    io,
    net::{
        Shutdown,
        SocketAddr,
    },
};

#[async_trait::async_trait]
pub trait Datagram: Sized {
    type Address;
    type Error = io::Error;

    async fn connect(address: &Self::Address) -> Result<Self, Self::Error>;
    fn shutdown(&self, how: Shutdown) -> Result<(), Self::Error>;
    fn display_addr(addr: &Self::Address) -> String;
}

#[async_trait::async_trait]
pub trait DatagramReceiver: Datagram {
    async fn recv(&self, packet: &mut [u8]) -> Result<usize, Self::Error>;
}

#[async_trait::async_trait]
pub trait DatagramSender: Datagram {
    async fn send(&self, packet: &[u8]) -> Result<usize, Self::Error>;
}

#[async_trait::async_trait]
impl Datagram for UdpSocket {
    type Address = SocketAddr;

    #[inline]
    async fn connect(address: &SocketAddr) -> io::Result<Self> {
        let sock = UdpSocket::bind("localhost:0").await?;
        sock.connect(address).await?;

        Ok(sock)
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

#[async_trait::async_trait]
impl DatagramSender for UdpSocket {
    #[inline]
    async fn send(&self, packet: &[u8]) -> io::Result<usize> {
        self.send(&packet).await
    }
}

#[async_trait::async_trait]
impl DatagramReceiver for UdpSocket {
    #[inline]
    async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.recv(buf).await
    }
}
