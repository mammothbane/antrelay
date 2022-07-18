use actix::prelude::*;

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, derive_more::Display)]
#[repr(u8)]
pub enum State {
    FlightIdle,
    PingCentralStation,
    StartBLE,
    PollAnt,
    CalibrateIMU,
    AntRun,
}

pub struct StateMachine {
    state: State,
}

impl Actor for StateMachine {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {}
}
