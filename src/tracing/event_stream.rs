use std::hash::{
    Hash,
    Hasher,
};

use tracing::{
    span::Record,
    Id,
    Metadata,
    Subscriber,
};
use tracing_subscriber::{
    layer::Context,
    registry::LookupSpan,
    Layer,
};

use crate::tracing::{
    RetrieveValues,
    Values,
};

#[derive(Clone, Debug)]
pub struct Event {
    pub location: Option<(String, u32)>,
    pub args:     Values,
    pub ty:       EventType,
}

impl Event {
    #[inline]
    pub fn location_hash(&self) -> u64 {
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

pub struct EventStream(smol::channel::Sender<Event>);

impl EventStream {
    #[inline]
    pub fn new(stream: smol::channel::Sender<Event>) -> Self {
        Self(stream)
    }
}

macro_rules! record_span {
    ($name:ident, $ty:expr) => {
        record_span!($name, &::tracing::Id, $ty);
    };

    ($name:ident, $idty:ty, $ty:expr) => {
        fn $name(&self, id: $idty, ctx: Context<'_, S>) {
            let _: Option<_> = try {
                let meta = ctx.metadata(&id)?;
                let values = ctx.span(&id)?.extensions().get::<$crate::tracing::Values>()?.clone();

                self.0
                    .try_send(Event {
                        location: Event::location_from_meta(meta),
                        args:     values,
                        ty:       $ty,
                    })
                    .unwrap()
            };
        }
    };
}

impl<S> Layer<S> for EventStream
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
        let mut values = values.values();

        if let Some(spanref) = ctx.span(id) {
            let mut extensions = spanref.extensions_mut();

            loop {
                match extensions.remove::<Values>() {
                    Some(cur_values) => values.extend(cur_values),
                    None => match extensions.replace(values) {
                        Some(replaced) => values = replaced,
                        None => break,
                    },
                }
            }
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let values = event.values();
        let meta = event.metadata();

        if let Err(e) = self.0.try_send(Event {
            location: Event::location_from_meta(meta),
            args:     values,
            ty:       EventType::Event,
        }) {
            eprintln!("failed to send event: {}", e);
        }
    }
}
