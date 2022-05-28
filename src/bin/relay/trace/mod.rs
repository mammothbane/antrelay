use std::{
    fmt::Debug,
    hash::Hash,
    str::FromStr,
};

use smol::stream::Stream;
use tracing::{
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
};

use crate::trace::visit::SpanValues;

pub mod visit;

#[derive(Clone, Debug)]
pub struct PortEvent {
    pub location: Option<(String, u32)>,
    pub args:     SpanValues,
    pub ty:       EventType,
}

impl PortEvent {
    #[inline]
    pub fn location_hash(&self) -> u64 {
        use std::hash::Hasher;

        let mut hasher = fnv::FnvHasher::default();
        self.location.hash(&mut hasher);

        hasher.finish()
    }

    fn location_from_meta(meta: &Metadata) -> Option<(String, u32)> {
        let line = meta.line()?;
        let file = meta.file()?.to_string();

        Some((file, line))
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub enum EventType {
    Event,
    SpanEnter,
    SpanExit,
    SpanClose,
}

pub fn init() -> eyre::Result<impl Stream<Item = PortEvent>> {
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

    let (tx, rx) = smol::channel::unbounded();

    tracing_subscriber::registry()
        .with(mk_level_filter())
        .with(stderr_layer)
        .with(DownlinkForwardLayer(tx))
        .init();

    Ok(rx)
}

fn mk_level_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let default_str = {
            cfg_if::cfg_if! {
                if #[cfg(not(debug_assertions))] {
                    "warn,relay=info"
                } else {
                    "info,relay=debug"
                }
            }
        };

        EnvFilter::from_str(default_str).expect("parsing envfilter default string")
    })
}

struct DownlinkForwardLayer(smol::channel::Sender<PortEvent>);

macro_rules! record_span {
    ($name:ident, $ty:expr) => {
        record_span!($name, &Id, $ty);
    };

    ($name:ident, $idty:ty, $ty:expr) => {
        fn $name(&self, id: $idty, ctx: Context<'_, S>) {
            let _: Option<_> = try {
                let meta = ctx.metadata(&id)?;
                let values = ctx.span(&id)?.extensions().get::<SpanValues>()?.clone();

                self.0
                    .try_send(PortEvent {
                        location: PortEvent::location_from_meta(meta),
                        args:     values,
                        ty:       $ty,
                    })
                    .unwrap()
            };
        }
    };
}

impl<S> Layer<S> for DownlinkForwardLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    record_span!(on_enter, EventType::SpanEnter);

    record_span!(on_exit, EventType::SpanExit);

    record_span!(on_close, Id, EventType::SpanClose);

    fn enabled(&self, metadata: &Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        metadata.fields().field("no_downlink").is_none()
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let values = visit::fields(values);

        if let Some(spanref) = ctx.span(id) {
            spanref.extensions_mut().insert(values);
        }
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let values = visit::event_fields(event);
        let meta = event.metadata();

        self.0
            .try_send(PortEvent {
                location: PortEvent::location_from_meta(meta),
                args:     values,
                ty:       EventType::Event,
            })
            .unwrap();
    }
}
