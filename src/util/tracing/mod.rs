use std::collections::HashMap;

mod event_stream;
mod visit;

pub use visit::RetrieveValues;

#[derive(Clone, Debug, PartialEq, PartialOrd, derive_more::From)]
pub enum Value {
    F64(f64),
    I64(i64),
    U64(u64),
    Bool(bool),
    String(String),
}

pub type Values = HashMap<&'static str, Value>;
