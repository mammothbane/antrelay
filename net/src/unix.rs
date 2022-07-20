use std::{
    io,
    net::Shutdown,
    path::PathBuf,
};

use tokio::net::UnixDatagram;

use crate::{
    DatagramOps,
    DatagramReceiver,
    DatagramSender,
};

#[async_trait::async_trait]
impl DatagramOps for UnixDatagram {
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
            tokio::fs::create_dir_all(parent).await?;
        }

        match tokio::fs::remove_file(address).await {
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
        let canonical = addr.canonicalize();

        canonical.as_ref().unwrap_or(addr).display().to_string()
    }
}

#[async_trait::async_trait]
impl DatagramSender for UnixDatagram {
    #[tracing::instrument(skip(self), err(Display))]
    #[inline]
    async fn send(&self, packet: &[u8]) -> io::Result<usize> {
        self.send(&packet).await
    }
}

#[async_trait::async_trait]
impl DatagramReceiver for UnixDatagram {
    #[tracing::instrument(skip(self), err(Display))]
    #[inline]
    async fn recv(&self, packet: &mut [u8]) -> io::Result<usize> {
        self.recv(packet).await
    }
}
