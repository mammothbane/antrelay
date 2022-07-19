use actix::{
    prelude::*,
    MessageResult,
};

use message::MissionEpoch;

#[derive(Message)]
#[rtype(result = "MissionEpoch")]
pub struct Request;

#[derive(Default)]
pub struct TimeService;

impl Actor for TimeService {
    type Context = Context<Self>;
}

impl Supervised for TimeService {}

impl SystemService for TimeService {}

impl Handler<Request> for TimeService {
    type Result = MessageResult<Request>;

    fn handle(&mut self, _msg: Request, _ctx: &mut Self::Context) -> Self::Result {
        MessageResult(MissionEpoch::now())
    }
}
