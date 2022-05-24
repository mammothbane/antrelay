use std::{
    hash::{
        Hash,
        Hasher,
    },
    str::FromStr,
};

use async_std::sync::Arc;
use smol::{
    net::unix::UnixDatagram,
    stream::Stream,
};
use tracing::{
    info_span,
    span::Record,
    Event,
    Id,
    Metadata,
    Subscriber,
};
use tracing_subscriber::{
    fmt::format::FmtSpan,
    layer::Context,
    prelude::*,
    registry::LookupSpan,
    EnvFilter,
    Layer,
    Registry,
};

pub fn init(
    downlink: Option<&crate::downlink::DownlinkSockets>,
) -> anyhow::Result<impl Stream<Item = lunarrelay::message::Downlink>> {
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

    let (tx, rx) = smol::channel::unbounded();

    let dgram_layers = downlink.map(
        move |crate::downlink::DownlinkSockets {
                  telemetry,
                  reliable,
                  store_forward,
              }| {
            let tel_layer = DatagramLayer::from(telemetry).with_filter(tel_filter);
            let reliable_layer = DatagramLayer::from(reliable).with_filter(rel_filter);
            let store_layer = DatagramLayer::from(store_forward).with_filter(snf_filter);

            store_layer.and_then(reliable_layer).and_then(tel_layer)
        },
    );

    tracing_subscriber::registry()
        .with(mk_level_filter())
        .with(stderr_layer)
        .with(dgram_layers)
        .init();

    Ok(rx)
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

impl<S> Layer<S> for DatagramLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn enabled(&self, metadata: &Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        metadata.fields().field("no_downlink").is_none()
    }

    fn on_record(&self, _span: &Id, _values: &Record<'_>, _ctx: Context<'_, S>) {}

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let meta = event.metadata();

        let location_hash = {
            use std::hash::Hasher;

            let mut hasher = fnv::FnvHasher::default();

            meta.line().map(|line| line as i64).unwrap_or(-1).hash(&mut hasher);

            let file = meta.file().unwrap_or("<no file>");
            file.hash(&mut hasher);

            hasher.finish()
        };
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let meta = ctx.metadata(id).unwrap();
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {}
}

impl From<&Arc<UnixDatagram>> for DatagramLayer {
    fn from(dgram: &Arc<UnixDatagram>) -> Self {
        Self(dgram.clone())
    }
}
