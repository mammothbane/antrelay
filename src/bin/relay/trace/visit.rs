use std::{
    collections::HashMap,
    error::Error,
    fmt::Debug,
};

use tracing::{
    span::Record,
    Event,
};

#[derive(Clone, Debug, PartialEq, PartialOrd, derive_more::From)]
pub enum Value {
    F64(f64),
    I64(i64),
    U64(u64),
    Bool(bool),
    String(String),
}

pub type SpanValues = HashMap<&'static str, Value>;

struct Visitor(SpanValues);

macro_rules! imp {
    ($name:ident, $ty:ty, $expr:expr) => {
        fn $name(&mut self, field: &::tracing::field::Field, value: $ty) {
            self.0.insert(field.name(), Value::from($expr(value)));
        }
    };
}

impl tracing_subscriber::field::Visit for Visitor {
    imp!(record_f64, f64, std::convert::identity);

    imp!(record_i64, i64, std::convert::identity);

    imp!(record_u64, u64, std::convert::identity);

    imp!(record_bool, bool, std::convert::identity);

    imp!(record_str, &str, std::string::ToString::to_string);

    imp!(record_error, &(dyn Error + 'static), |value| format!("{}", value));

    imp!(record_debug, &dyn Debug, |value| format!("{:?}", value));
}

pub fn event_fields(event: &Event) -> SpanValues {
    let mut visit = Visitor(HashMap::new());
    event.record(&mut visit);

    visit.0
}

pub fn fields(record: &Record) -> SpanValues {
    let mut visit = Visitor(HashMap::new());
    record.record(&mut visit);

    visit.0
}
