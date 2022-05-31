use std::collections::HashMap;

mod event_stream;
mod visit;

pub use event_stream::*;
pub use visit::RetrieveValues;

#[derive(Clone, Debug, PartialEq, PartialOrd, derive_more::From, derive_more::Display)]
pub enum Value {
    F64(f64),
    I64(i64),
    U64(u64),
    Bool(bool),
    String(String),
}

pub type Values = HashMap<&'static str, Value>;
