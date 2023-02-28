use std::{
    any::{
        Any,
        TypeId,
    },
    sync::Arc,
};

use actix::{
    prelude::*,
    Actor,
    Context,
    Supervised,
    SystemService,
};

#[derive(Message)]
#[rtype(result = "()")]
struct Set(TypeId, Arc<dyn Any + Send + Sync>);

#[derive(Message)]
#[rtype(result = "Option<Arc<dyn Any + Send + Sync>>")]
struct Query(TypeId);

#[derive(Message)]
#[rtype(result = "()")]
struct Reset(TypeId);

#[derive(Default)]
pub struct OverrideRegistry {
    map: fnv::FnvHashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl OverrideRegistry {
    pub async fn set<M, A>()
    where
        M: Message + Send + 'static,
        M::Result: Send,
        A: Handler<M> + SystemService,
    {
        OverrideRegistry::from_registry()
            .send(Set(TypeId::of::<M>(), Arc::new(A::from_registry().recipient())))
            .await
            .unwrap();
    }

    pub async fn reset<M>()
    where
        M: Message + 'static,
    {
        OverrideRegistry::from_registry().send(Reset(TypeId::of::<M>())).await.unwrap();
    }

    pub async fn query<M, Fallback>() -> Recipient<M>
    where
        M: Message + Send + Sync + 'static,
        M::Result: Send,
        Fallback: Handler<M> + SystemService,
    {
        let stored_result: Option<Recipient<M>> = try {
            let resp: Arc<dyn Any + Send + Sync + 'static> =
                OverrideRegistry::from_registry().send(Query(TypeId::of::<M>())).await.unwrap()?;
            let ret = resp.downcast_ref::<Recipient<M>>()?;

            ret.clone()
        };

        match stored_result {
            Some(recip) => recip,
            None => Fallback::from_registry().recipient(),
        }
    }
}

impl Actor for OverrideRegistry {
    type Context = Context<Self>;
}

impl Supervised for OverrideRegistry {}

impl SystemService for OverrideRegistry {}

impl Handler<Set> for OverrideRegistry {
    type Result = MessageResult<Set>;

    fn handle(&mut self, Set(type_id, recip): Set, _ctx: &mut Self::Context) -> Self::Result {
        self.map.insert(type_id, recip);

        MessageResult(())
    }
}

impl Handler<Reset> for OverrideRegistry {
    type Result = MessageResult<Reset>;

    fn handle(&mut self, Reset(type_id): Reset, _ctx: &mut Self::Context) -> Self::Result {
        self.map.remove(&type_id);

        MessageResult(())
    }
}

impl Handler<Query> for OverrideRegistry {
    type Result = MessageResult<Query>;

    fn handle(&mut self, Query(type_id): Query, _ctx: &mut Self::Context) -> Self::Result {
        let result = self.map.get(&type_id).cloned();

        MessageResult(result)
    }
}
