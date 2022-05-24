use std::{
    net::Shutdown,
    path::Path,
    time::Duration,
};

use async_std::sync::Arc;
use futures::AsyncRead;
use smol::{
    io::AsyncBufReadExt,
    net::unix::UnixDatagram,
};

use crate::options::Options;
use lunarrelay::{
    message,
    util,
};

#[derive(Clone)]
pub struct DownlinkSockets {
    pub telemetry:     Arc<UnixDatagram>,
    pub reliable:      Arc<UnixDatagram>,
    pub store_forward: Arc<UnixDatagram>,
}

impl DownlinkSockets {
    fn try_new(
        telemetry: impl AsRef<Path>,
        reliable: impl AsRef<Path>,
        store_forward: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            telemetry:     Arc::new(util::uds_connect(telemetry)?),
            reliable:      Arc::new(util::uds_connect(reliable)?),
            store_forward: Arc::new(util::uds_connect(store_forward)?),
        })
    }

    pub async fn send_all(&self, data: &[u8]) -> anyhow::Result<()> {
        let (r1, r2, r3) = futures::future::join3(
            self.telemetry.send(data),
            self.reliable.send(data),
            self.store_forward.send(data),
        )
        .await;

        r1.or(r2).or(r3)?;
        Ok(())
    }

    pub fn shutdown(&self, how: Shutdown) -> anyhow::Result<()> {
        let r1 = self.telemetry.shutdown(how);
        let r2 = self.reliable.shutdown(how);
        let r3 = self.store_forward.shutdown(how);

        r1.or(r2).or(r3)?;
        Ok(())
    }
}

impl TryFrom<&Options> for DownlinkSockets {
    type Error = anyhow::Error;

    fn try_from(opt: &Options) -> anyhow::Result<Self> {
        Self::try_new(&opt.telemetry_sock, &opt.reliable_sock, &opt.store_and_forward_sock)
    }
}

pub async fn downlink(
    downlink_sockets: DownlinkSockets,
    serial_read: impl AsyncRead + Unpin,
    done: smol::channel::Receiver<!>,
) {
    let mut buf = vec![0; 4096];

    let mut serial_read = smol::io::BufReader::new(serial_read);

    loop {
        let result: anyhow::Result<_> = try {
            // todo: framing pending fz spec
            let count = match util::either(serial_read.read_until(b'\n', &mut buf), done.recv())
                .await
            {
                either::Left(ret) => ret?,
                either::Right(_) => {
                    if let Err(e) = downlink_sockets.shutdown(Shutdown::Both) {
                        tracing::error!(error = ?e, "shutting down downlink sockets after done signal");
                    }

                    return;
                },
            };

            let data: &[u8] = &buf[..count];
            let framed_message = message::Downlink::Data(data);

            let mut framed_bytes = postcard::to_allocvec_cobs(&framed_message)?;

            let compressed_bytes = {
                let mut out = vec![];

                lazy_static::lazy_static! {
                    static ref PARAMS: brotli::enc::BrotliEncoderParams = brotli::enc::BrotliEncoderParams {
                        quality: 11,
                        ..Default::default()
                    };
                }

                brotli::BrotliCompress(&mut &framed_bytes[..], &mut out, &*PARAMS)?;

                out
            };

            downlink_sockets.send_all(&compressed_bytes).await?;

            // TODO: what do we actually want the strategy to be here re: where to send what?
        };

        if let Err(e) = result {
            tracing::error!(error = ?e, "downlink error, sleeping before retry");
            smol::Timer::after(Duration::from_millis(20)).await;
        }
    }
}
