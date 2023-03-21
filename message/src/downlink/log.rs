use std::{
    collections::BTreeMap,
    fmt::{
        Display,
        Formatter,
    },
};

use crate::downlink::Value;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Log(pub Vec<SpanData>);

impl Display for Log {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for SpanData {
            name,
            target,
            fields,
            level,
        } in &self.0
        {
            write!(f, "| {target} ({name}) [{level:?}] ")?;

            for (k, v) in fields {
                write!(f, "{k}: {v}, ")?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SpanData {
    pub name:   String,
    pub target: String,
    pub fields: BTreeMap<String, Value>,
    pub level:  Level,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Level {
    ERROR,
    WARN,
    INFO,
    DEBUG,
    TRACE,
    UNKNOWN,
}

impl From<&tracing::Level> for Level {
    #[inline]
    fn from(&value: &tracing::Level) -> Self {
        match value {
            tracing::Level::DEBUG => Level::DEBUG,
            tracing::Level::WARN => Level::WARN,
            tracing::Level::INFO => Level::INFO,
            tracing::Level::TRACE => Level::TRACE,
            tracing::Level::ERROR => Level::ERROR,
        }
    }
}
