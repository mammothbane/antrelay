use std::{
    collections::BTreeMap,
    sync::atomic::{
        AtomicBool,
        Ordering,
    },
};

use message::downlink::Value;

use tap::Pipe;

pub const LIMITED_DOWNLINK: &str = "limit_downlink";
pub const MAX_STR: usize = 64;
pub static ACTIVE: AtomicBool = AtomicBool::new(false);

pub struct Layer;

#[derive(Debug)]
struct FieldStorage(BTreeMap<String, Value>);

impl<S> tracing_subscriber::Layer<S> for Layer
where
    S: tracing::Subscriber,
    S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let span = ctx.span(id).unwrap();

        let mut fields = BTreeMap::new();

        let mut visitor = Visitor(&mut fields);
        attrs.record(&mut visitor);

        let mut extensions = span.extensions_mut();
        extensions.insert(FieldStorage(fields));
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        if !ACTIVE.load(Ordering::SeqCst) {
            return;
        }

        let mut fields = BTreeMap::new();

        let mut visitor = Visitor(&mut fields);
        event.record(&mut visitor);

        let record_field = fields.get(LIMITED_DOWNLINK).unwrap_or(&Value::Bool(false));
        let limited_downlink = matches!(record_field, &Value::Bool(true));

        let mut spans = vec![];

        let mut record =
            |mut fields: BTreeMap<String, Value>, name: &str, meta: &tracing::Metadata| {
                if limited_downlink {
                    let msg = fields.remove("message");
                    fields.clear();

                    if let Some(msg) = msg {
                        fields.insert("message".to_string(), msg);
                        fields.insert("downlink_filtered".to_string(), Value::Bool(true));
                    }
                }

                spans.push(message::downlink::log::SpanData {
                    target: meta.target().pipe(truncate).to_string(),
                    name: name.pipe(truncate).to_string(),
                    level: meta.level().into(),
                    fields,
                });
            };

        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                let extensions = span.extensions();
                let storage = extensions.get::<FieldStorage>().unwrap();

                record(storage.0.clone(), span.name(), span.metadata());
            }
        }

        let meta = event.metadata();
        record(fields, meta.name(), meta);

        actix_broker::Broker::<actix_broker::SystemBroker>::issue_async(runtime::ground::Log(
            message::downlink::log::Log(spans),
        ));
    }
}

struct Visitor<'a>(&'a mut BTreeMap<String, Value>);

impl<'a> tracing::field::Visit for Visitor<'a> {
    #[inline]
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0.insert(field.name().pipe(truncate).to_string(), Value::F64(value));
    }

    #[inline]
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0.insert(field.name().pipe(truncate).to_string(), Value::I64(value));
    }

    #[inline]
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.insert(field.name().pipe(truncate).to_string(), Value::U64(value));
    }

    #[inline]
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0.insert(field.name().pipe(truncate).to_string(), Value::Bool(value));
    }

    #[inline]
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.insert(
            field.name().pipe(truncate).to_string(),
            Value::String(value.pipe(truncate).to_string()),
        );
    }

    #[inline]
    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.record_str(field, &value.to_string());
    }

    #[inline]
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.record_str(field, &format!("{value:?}"));
    }
}

#[inline]
fn truncate(s: &str) -> &str {
    if s.is_empty() {
        return s;
    }

    let (idx, _) = s.char_indices().take(MAX_STR).last().unwrap_or((0, '\0'));

    &s[..=idx]
}
