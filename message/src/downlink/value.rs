use std::fmt::{
    Display,
    Formatter,
};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Value {
    String(String),
    U64(u64),
    I64(i64),
    F64(f64),
    Bool(bool),
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use Value::*;

        match self {
            String(elem) => write!(f, "{elem}"),
            U64(elem) => write!(f, "{elem}"),
            I64(elem) => write!(f, "{elem}"),
            F64(elem) => write!(f, "{elem}"),
            Bool(elem) => write!(f, "{elem}"),
        }
    }
}
