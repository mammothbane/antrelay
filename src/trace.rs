use std::{
    str::FromStr,
};

use async_std::sync::{Arc};
use smol::{
    net::unix::UnixDatagram,
};
use tracing::{
    Event,
    Id,
    Metadata,
    Subscriber,
};
use tracing_subscriber::{
    layer::{
        Context,
    },
    prelude::*,
    EnvFilter,
    Layer,
};
use tracing_subscriber::fmt::format::FmtSpan;

pub fn init(downlink: Option<&crate::downlink::DownlinkSockets>) -> anyhow::Result<()> {
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_span_events(FmtSpan::CLOSE);

    let stderr_layer = {
        cfg_if::cfg_if! {
            if #[cfg(debug_assertions)] {
                stderr_layer.pretty()
            } else {
                stderr_layer.json()
            }
        }
    };

    let tel_filter = EnvFilter::try_new("[{telemetry}],[{downlink}]")?;
    let rel_filter = EnvFilter::try_new("[{reliable}],[{downlink}]")?;
    let snf_filter = EnvFilter::try_new("[{store_forward}],[{downlink}]")?;

    let dgram_layers = downlink.map(
        move |crate::downlink::DownlinkSockets {
             telemetry,
             reliable,
             store_forward,
         }| {
            let tel_layer = DatagramLayer::from(telemetry)
                .with_filter(tel_filter);

            let reliable_layer = DatagramLayer::from(reliable)
                .with_filter(rel_filter);

            let store_layer = DatagramLayer::from(store_forward)
                .with_filter(snf_filter);

            store_layer
                .and_then(reliable_layer)
                .and_then(tel_layer)
        },
    );

    tracing_subscriber::registry()
        .with(mk_level_filter())
        .with(stderr_layer)
        .with(dgram_layers)
        .init();

    Ok(())
}

fn mk_level_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let default_str = {
            cfg_if::cfg_if! {
                if #[cfg(not(debug_assertions))] {
                    "warn,lunarrelay=info"
                } else {
                    "info,lunarrelay=debug"
                }
            }
        };

        EnvFilter::from_str(default_str).expect("parsing envfilter default string")
    })
}

struct DatagramLayer(Arc<UnixDatagram>);

impl<S: Subscriber> Layer<S> for DatagramLayer {
    fn enabled(&self, metadata: &Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        metadata.fields().field("no_downlink").is_none()
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let fields = event.fields();
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        smol::block_on(async move {
            // sock.send()
        })
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {}
}

impl From<&Arc<UnixDatagram>> for DatagramLayer {
    fn from(dgram: &Arc<UnixDatagram>) -> Self {
        Self(dgram.clone())
    }
}
