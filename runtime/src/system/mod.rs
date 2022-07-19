mod override_registry;
mod seq_service;
mod time_service;

pub use override_registry::OverrideRegistry;

use message::MissionEpoch;

pub async fn time() -> MissionEpoch {
    OverrideRegistry::query::<time_service::Request, time_service::TimeService>()
        .await
        .send(time_service::Request)
        .await
        .unwrap()
}

pub async fn seq() -> u8 {
    OverrideRegistry::query::<seq_service::Request, seq_service::SeqService>()
        .await
        .send(seq_service::Request)
        .await
        .unwrap()
}

pub async fn params() -> message::Params {
    let (time, seq) = futures::future::join(time(), seq()).await;

    message::Params {
        time,
        seq,
    }
}
