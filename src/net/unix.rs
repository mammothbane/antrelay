use std::{
    io,
    net::Shutdown,
    path::PathBuf,
};

use smol::net::unix::UnixDatagram;

use crate::net::{
    Datagram,
    DatagramReceiver,
    DatagramSender,
};

#[async_trait::async_trait]
impl Datagram for UnixDatagram {
    type Address = PathBuf;

    #[inline]
    #[tracing::instrument(fields(address = ?address), err(Display))]
    async fn connect(address: &PathBuf) -> io::Result<Self> {
        let sock = UnixDatagram::unbound()?;
        sock.connect(address)?;

        Ok(sock)
    }

    #[inline]
    #[tracing::instrument(fields(address = ?address), err(Display))]
    async fn bind(address: &Self::Address) -> Result<Self, Self::Error> {
        if let Some(parent) = address.parent() {
            smol::fs::create_dir_all(parent).await?;
        }

        match smol::fs::remove_file(address).await {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {},
            result => result?,
        };

        let result = UnixDatagram::bind(address)?;

        Ok(result)
    }

    #[inline]
    fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        UnixDatagram::shutdown(self, how)
    }

    #[inline]
    fn display_addr(addr: &PathBuf) -> String {
        addr.display().to_string()
    }
}

#[async_trait::async_trait]
impl DatagramSender for UnixDatagram {
    #[inline]
    async fn send(&self, packet: &[u8]) -> io::Result<usize> {
        self.send(&packet).await
    }
}

#[async_trait::async_trait]
impl DatagramReceiver for UnixDatagram {
    #[inline]
    async fn recv(&self, packet: &mut [u8]) -> io::Result<usize> {
        self.recv(packet).await
    }
}
