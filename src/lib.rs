use std::{
    sync::atomic::{
        AtomicBool,
        Ordering,
    },
    time::Duration,
};

use net::DatagramOps;

#[cfg(windows)]
pub type Socket = tokio::net::UdpSocket;

#[cfg(unix)]
pub type Socket = tokio::net::UnixDatagram;

pub type Address = <Socket as DatagramOps>::Address;

pub async fn connect_once(addrs: &[Address]) {
    let has_notified = AtomicBool::new(false);

    tracing::info!("waiting for downlink sockets to become available");

    tokio_retry::Retry::spawn(
        tokio_retry::strategy::ExponentialBackoff::from_millis(100)
            .max_delay(Duration::from_secs(3)),
        || {
            let has_notified = &has_notified;
            let addrs = addrs.iter();

            async move {
                let mut ok = false;

                for addr in addrs {
                    if let Ok(_) = <Socket as DatagramOps>::connect(addr).await {
                        ok = true;
                        break;
                    }
                }

                if !ok && !has_notified.load(Ordering::SeqCst) {
                    has_notified.store(true, Ordering::SeqCst);
                    tracing::error!("failed to connect to any downlink socket, retrying...");
                }

                return if ok {
                    Ok(())
                } else {
                    tracing::trace!("no connection to socket(s)");
                    Err(())
                };
            }
        },
    )
    .await
    .unwrap();

    tracing::info!("at least one socket present");
}
