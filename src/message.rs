#[derive(Debug, Clone, Hash, serde::Serialize)]
#[serde(tag = "ty")]
pub enum Downlink<'a> {
    Data(&'a [u8]),
}
