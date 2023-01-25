use crate::{
    BytesWrap,
    Message,
};

/// All the message types we'll send back over the downlink.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Downlink {
    Log(String),

    UplinkMirror(BytesWrap),
    UplinkInterpreted(Message),

    SerialUplink(Message),
    SerialDownlink(Message),

    SerialUplinkRaw(BytesWrap),
    SerialDownlinkRaw(BytesWrap),
}
