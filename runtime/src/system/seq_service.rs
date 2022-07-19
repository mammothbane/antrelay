use std::sync::atomic::{
    AtomicU8,
    Ordering,
};

use actix::prelude::*;

#[derive(Message)]
#[rtype(result = "u8")]
pub struct Request;

#[derive(Default)]
pub struct SeqService(AtomicU8);

impl Actor for SeqService {
    type Context = Context<Self>;
}

impl Supervised for SeqService {}

impl SystemService for SeqService {}

impl Handler<Request> for SeqService {
    type Result = MessageResult<Request>;

    #[inline]
    fn handle(&mut self, _msg: Request, _ctx: &mut Self::Context) -> Self::Result {
        MessageResult(self.0.fetch_add(1, Ordering::SeqCst))
    }
}
