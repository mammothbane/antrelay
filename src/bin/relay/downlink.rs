use std::{
    net::Shutdown,
    path::Path,
    time::Duration,
};

use async_std::sync::Arc;
use futures::AsyncRead;
use packed_struct::PackedStructSlice;
use smol::{
    io::AsyncBufReadExt,
    net::unix::UnixDatagram,
};

use crate::options::Options;
use lunarrelay::{
    message,
    util,
};

const COBS_SENTINEL: u8 = 0;

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
    ) -> eyre::Result<Self> {
        Ok(Self {
            telemetry:     Arc::new(util::uds_connect(telemetry)?),
            reliable:      Arc::new(util::uds_connect(reliable)?),
            store_forward: Arc::new(util::uds_connect(store_forward)?),
        })
    }

    pub async fn send_all(&self, data: &[u8]) -> eyre::Result<()> {
        let (r1, r2, r3) = futures::future::join3(
            self.telemetry.send(data),
            self.reliable.send(data),
            self.store_forward.send(data),
        )
        .await;

        r1.or(r2).or(r3)?;
        Ok(())
    }

    pub fn shutdown(&self, how: Shutdown) -> eyre::Result<()> {
        let r1 = self.telemetry.shutdown(how);
        let r2 = self.reliable.shutdown(how);
        let r3 = self.store_forward.shutdown(how);

        r1.or(r2).or(r3)?;
        Ok(())
    }
}

impl TryFrom<&Options> for DownlinkSockets {
    type Error = eyre::Error;

    fn try_from(opt: &Options) -> eyre::Result<Self> {
        Self::try_new(&opt.telemetry_sock, &opt.reliable_sock, &opt.store_and_forward_sock)
    }
}

#[derive(Copy, Clone, Debug)]
enum DownlinkStatus {
    Continue,
    Close,
}

#[tracing::instrument(skip_all)]
pub async fn downlink(
    downlink_sockets: DownlinkSockets,
    serial_read: impl AsyncRead + Unpin,
    done: smol::channel::Receiver<!>,
) {
    let mut buf = vec![0; 4096];
    let mut serial_read = smol::io::BufReader::new(serial_read);

    loop {
        match downlink_once(&downlink_sockets, &mut serial_read, &mut buf, &done).await {
            Ok(DownlinkStatus::Continue) => {},
            Ok(DownlinkStatus::Close) => break,
            Err(e) => {
                tracing::error!(error = ?e, "downlink error, sleeping before retry");
                smol::Timer::after(Duration::from_millis(20)).await;
            },
        }
    }
}

async fn downlink_once(
    downlink_sockets: &DownlinkSockets,
    serial_read: &mut (impl AsyncBufReadExt + Unpin),
    buf: &mut Vec<u8>,
    done: &smol::channel::Receiver<!>,
) -> eyre::Result<DownlinkStatus> {
    // todo: framing pending fz spec
    let count = match util::either(serial_read.read_until(COBS_SENTINEL, buf), done.recv()).await {
        either::Left(ret) => ret?,
        either::Right(_) => {
            if let Err(e) = downlink_sockets.shutdown(Shutdown::Both) {
                tracing::error!(error = ?e, "shutting down downlink sockets after done signal");
            }

            return Ok(DownlinkStatus::Close);
        },
    };

    let (data, rest) = postcard::take_from_bytes_cobs::<Vec<u8>>(&mut buf[..count])?;
    debug_assert_eq!(rest.len(), 0);

    let message = message::Message {
        header:  todo!(),
        payload: message::Payload::new(data),
    };

    let packed_message = message.pack_to_vec()?;

    let mut framed_bytes = postcard::to_allocvec(&packed_message)?;

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

    Ok(DownlinkStatus::Continue)
}
