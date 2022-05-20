#[derive(Debug, Clone, Hash, Serialize)]
#[serde(tag = "ty")]
pub enum DownlinkMessage {
    Log {
        level: log::Level,
    },
    Data(Vec<u8>),
}
