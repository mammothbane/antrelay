use actix::Message;
use bytes::Bytes;

mod serial;
mod state_machine;
mod time_service;

pub use state_machine::StateMachine;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Message)]
#[rtype(result = "()")]
pub struct GroundUplink(pub Bytes);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Message)]
#[rtype(result = "()")]
pub struct GroundDownlink(pub Bytes);
