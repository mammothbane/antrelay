use actix::Message;
use bytes::Bytes;

pub mod downlink;
pub mod uplink;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Message)]
#[rtype(result = "()")]
pub struct UpPacket(pub Bytes);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Message)]
#[rtype(result = "()")]
pub struct DownPacket(pub Bytes);

#[derive(Clone, Debug, PartialEq, Message)]
#[rtype(result = "()")]
pub struct UpCommand(pub message::Message);
