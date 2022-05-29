use std::{
    io,
    net::Shutdown,
    path::{
        Path,
        PathBuf,
    },
};

use smol::net::unix::UnixDatagram;

use crate::util::net::{
    Datagram,
    DatagramReceiver,
    DatagramSender,
};

pub mod dynload;

pub fn signals() -> eyre::Result<smol::channel::Receiver<!>> {
    use signal_hook::{
        consts::*,
        iterator::Signals,
    };
    use std::thread;

    let (tx, rx) = smol::channel::bounded(1);
    let mut signals = Signals::new(&[SIGTERM, SIGINT])?;

    thread::spawn(move || {
        signals.wait().next();

        tx.close();
    });

    Ok(rx)
}

#[tracing::instrument(fields(path = ?p.as_ref()), skip(p), err(Display))]
pub fn uds_connect(p: impl AsRef<Path>) -> io::Result<UnixDatagram> {
    let sock = UnixDatagram::unbound()?;
    sock.connect(p.as_ref())?;

    Ok(sock)
}

#[tracing::instrument(fields(path = ?p.as_ref()), skip(p), err(Display))]
pub async fn remove_and_bind(p: impl AsRef<Path>) -> io::Result<UnixDatagram> {
    match smol::fs::remove_file(&p).await {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {},
        result => result?,
    };

    let result = UnixDatagram::bind(&p)?;

    Ok(result)
}

#[async_trait::async_trait]
impl Datagram for UnixDatagram {
    type Address = PathBuf;

    #[inline]
    async fn connect(address: &PathBuf) -> io::Result<Self> {
        uds_connect(address)
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
