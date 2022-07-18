use std::hash::Hash;

use actix_broker::{
    ArbiterBroker,
    Broker,
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

use crate::{
    RetrieveValues,
    Value,
    Values,
};

#[derive(
    Clone, Debug, derive_more::Display, serde::Serialize, serde::Deserialize, actix::Message,
)]
#[display(fmt = "[{}] {} (values: {:?})", ty, "self.message()", args)]
#[rtype(response = "()")]
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

macro_rules! record_span {
    ($name:ident, $ty:expr) => {
        record_span!($name, &::tracing::Id, $ty);
    };

    ($name:ident, $idty:ty, $ty:expr) => {
        fn $name(&self, id: $idty, ctx: Context<'_, S>) {
            let _: Option<_> = try {
                let values = ctx.span(&id)?.extensions().get::<$crate::Values>()?.clone();

                ::actix_broker::Broker::<::actix_broker::ArbiterBroker>::issue_async(Event {
                    args: values,
                    ty:   $ty,
                });
            };
        }
    };
}

#[derive(Default)]
pub struct EventStream;

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
        Broker::<ArbiterBroker>::issue_async(Event {
            args: event.values(),
            ty:   EventType::Event,
        });
    }
}
