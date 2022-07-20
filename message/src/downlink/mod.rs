use crate::{
    BytesWrap,
    Message,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Downlink {
    Log(),

    UplinkMirror(BytesWrap),
    UplinkInterpreted(Message),

    SerialUplink(Message),
    SerialDownlink(Message),

    SerialUplinkRaw(BytesWrap),
    SerialDownlinkRaw(BytesWrap),

    Direct(BytesWrap),
}
