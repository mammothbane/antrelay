use actix::prelude::*;

use message::MissionEpoch;

#[derive(Message)]
#[rtype = "MissionEpoch"]
pub struct Request;

#[derive(Default)]
pub struct TimeService;

impl Actor for TimeService {
    type Context = Context<Self>;
}

impl Supervised for TimeService {}

impl SystemService for TimeService {}

impl Handler<Request> for TimeService {
    type Result = MissionEpoch;

    fn handle(&mut self, _msg: Request, _ctx: &mut Self::Context) -> Self::Result {
        MissionEpoch::now()
    }
}
