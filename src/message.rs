#[derive(Debug, Clone, Hash, serde::Serialize)]
#[serde(tag = "t")]
pub enum Downlink<'a> {
    Data(&'a [u8]),
    Log {
        message:       String,
        location_hash: u64,
        arguments:     HashMap<String, serde_json::Value>,
    },
}
