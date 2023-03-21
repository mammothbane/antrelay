use std::fmt::{
    Debug,
    Display,
    Formatter,
};

use crate::{
    BytesWrap,
    Message,
};

/// All the message types we'll send back over the downlink.
#[derive(Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Downlink {
    Log(String),

    UplinkMirror(BytesWrap),
    UplinkInterpreted(Message),

    SerialUplink(Message),
    SerialDownlink(Message),

    SerialUplinkRaw(BytesWrap),
    SerialDownlinkRaw(BytesWrap),
}

impl Display for Downlink {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use Downlink::*;

        match self {
            Log(s) => write!(f, "log: {s:?}"),

            UplinkMirror(b) => write!(f, "raw uplink: {b}"),
            UplinkInterpreted(m) => write!(f, "uplink: {m}"),

            SerialUplink(m) => write!(f, "serial up: {m}"),
            SerialDownlink(m) => write!(f, "serial down: {m}"),

            SerialUplinkRaw(b) => write!(f, "raw serial up: {b}"),
            SerialDownlinkRaw(b) => write!(f, "raw serial down: {b}"),
        }
    }
}

impl Debug for Downlink {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use Downlink::*;

        match self {
            Log(s) => write!(f, "Log({s})"),

            UplinkMirror(b) => write!(f, "UplinkMirror({b})"),
            UplinkInterpreted(m) => write!(f, "Uplink({m:?})"),

            SerialUplink(m) => write!(f, "SerialUplink({m:?})"),
            SerialDownlink(m) => write!(f, "SerialDownlink({m:?})"),

            SerialUplinkRaw(b) => write!(f, "SerialUplinkRaw({b})"),
            SerialDownlinkRaw(b) => write!(f, "SerialDownlinkRaw({b})"),
        }
    }
}
