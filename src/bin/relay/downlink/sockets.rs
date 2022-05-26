use std::{
    net::Shutdown,
    path::Path,
    sync::Arc,
};

use smol::net::unix::UnixDatagram;

use crate::Options;
use lunarrelay::util;

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
