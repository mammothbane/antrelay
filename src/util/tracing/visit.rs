use std::{
    collections::HashMap,
    error::Error,
    fmt::Debug,
};

use tracing::{
    span::Record,
    Event,
};

use crate::util::tracing::Values;

pub trait RetrieveValues {
    fn values(&self) -> Values;
}

impl<'a> RetrieveValues for Event<'a> {
    fn values(&self) -> Values {
        let mut visit = Visitor(HashMap::new());
        self.record(&mut visit);

        visit.0
    }
}

impl<'a> RetrieveValues for Record<'a> {
    fn values(&self) -> Values {
        let mut visit = Visitor(HashMap::new());
        self.record(&mut visit);

        visit.0
    }
}

struct Visitor(Values);

macro_rules! imp {
    ($name:ident, $ty:ty, $expr:expr) => {
        fn $name(&mut self, field: &::tracing::field::Field, value: $ty) {
            self.0.insert(field.name(), $crate::util::tracing::Value::from($expr(value)));
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
