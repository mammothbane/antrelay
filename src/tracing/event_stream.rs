use std::hash::Hash;

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
    Value,
    Values,
};

#[derive(Clone, Debug, derive_more::Display, serde::Serialize, serde::Deserialize)]
#[display(fmt = "[{}] {} (values: {:?})", ty, "self.message()", args)]
pub struct Event {
    pub args: Values,
    pub ty:   EventType,
}

impl Event {
    pub fn message(&self) -> &Value {
        self.args.get("message").unwrap_or(&Value::Bool(false))
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Hash,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    derive_more::Display,
)]
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
                let values = ctx.span(&id)?.extensions().get::<$crate::tracing::Values>()?.clone();

                self.0
                    .try_send(Event {
                        args: values,
                        ty:   $ty,
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
        if let Err(e) = self.0.try_send(Event {
            args: event.values(),
            ty:   EventType::Event,
        }) {
            eprintln!("failed to send event: {}", e);
        }
    }
}
