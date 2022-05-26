use futures::TryStreamExt;
use std::time::Duration;

use smol::stream::{
    Stream,
    StreamExt,
};

use lunarrelay::{
    message,
    util,
};

pub async fn relay(
    uplink: impl Stream<Item = message::Message>,
    serial_read: impl Stream<Item = message::Message>,
) -> (impl Stream<Item = message::Message>, impl Stream<Item = message::Message>) {
    let (downlink_tx, downlink_rx) = smol::channel::bounded(16);
    let (serial_tx, serial_rx) = smol::channel::bounded(16);

    let (response_tx, response_rx) = async_broadcast::broadcast(32);

    let circuit_breaker = mk_cb();

    let run_uplink = uplink
        .then(|msg| dispatch_request(msg, &circuit_breaker, response_rx.clone()))
        .try_for_each()
        .for_each(|r| r.unwrap());

    (downlink_rx, serial_rx)
}

#[tracing::instrument(fields(msg = ?msg), skip(cb, msgs))]
pub async fn dispatch_request(
    msg: message::Message,
    cb: &impl failsafe::futures::CircuitBreaker,
    mut msgs: async_broadcast::Receiver<message::Message>,
) -> std::result::Result<message::Message, failsafe::Error<eyre::Report>> {
    let hdr = msg.header;

    loop {
        let result = cb
            .call({
                let msgs = &mut msgs;

                async move {
                    match util::either(
                        msgs.find(|target_msg| {
                            target_msg.header._timestamp >= hdr._timestamp
                                && target_msg.header.seq == target_msg.header.seq
                                && target_msg.header.ty == hdr.ty
                        }),
                        smol::Timer::after(Duration::from_millis(500)),
                    )
                    .await
                    {
                        either::Left(find_result) => {
                            find_result.ok_or_else(|| eyre::eyre!("no response"))
                        },
                        either::Right(_timeout) => Err(eyre::eyre!("timed out")),
                    }
                }
            })
            .await;

        match result {
            Err(failsafe::Error::Rejected) | Ok(_) => return result,
            Err(e) => {
                tracing::error!(nonfatal = true, error = %e, "awaiting ack");
            },
        }
    }
}

#[inline]
fn mk_cb() -> impl failsafe::CircuitBreaker + failsafe::futures::CircuitBreaker {
    let policy = failsafe::failure_policy::consecutive_failures(
        10,
        failsafe::backoff::equal_jittered(Duration::from_millis(50), Duration::from_millis(2500)),
    );

    failsafe::Config::new().failure_policy(policy).build()
}
