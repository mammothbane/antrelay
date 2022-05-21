use std::path::Path;
use std::net::Shutdown;

use async_std::sync;

use smol::net::unix::UnixDatagram;

use crate::options::Options;
use crate::util;

#[derive(Clone)]
pub struct DownlinkSockets {
    pub telemetry:     sync::Arc<UnixDatagram>,
    pub reliable:      sync::Arc<UnixDatagram>,
    pub store_forward: sync::Arc<UnixDatagram>,
}

impl DownlinkSockets {
    fn try_new(
        telemetry: impl AsRef<Path>,
        reliable: impl AsRef<Path>,
        store_forward: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            telemetry:     sync::Arc::new(util::uds_connect(telemetry)?),
            reliable:      sync::Arc::new(util::uds_connect(reliable)?),
            store_forward: sync::Arc::new(util::uds_connect(store_forward)?),
        })
    }

    pub async fn send_all(&self, data: &[u8]) -> anyhow::Result<()> {
        let (r1, r2, r3) = futures::future::join3(
            self.telemetry.send(data),
            self.reliable.send(data),
            self.store_forward.send(data),
        ).await;

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
