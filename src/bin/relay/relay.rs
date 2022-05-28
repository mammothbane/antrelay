use std::time::Duration;

use smol::stream::{
    Stream,
    StreamExt,
};

use lunarrelay::{
    message::{
        payload,
        Message,
        OpaqueBytes,
    },
    util,
};

pub async fn relay(
    uplink: impl Stream<Item = Message<OpaqueBytes>>,
    serial_read: impl Stream<Item = Message<payload::Ack>>,
) -> (impl Stream<Item = Message<OpaqueBytes>>, impl Stream<Item = Message<payload::Ack>>) {
    let (downlink_tx, downlink_rx) = smol::channel::bounded(16);
    let (serial_tx, serial_rx) = smol::channel::bounded(16);

    let (response_tx, response_rx) = async_broadcast::broadcast(32);

    let circuit_breaker = mk_cb();

    let run_uplink = uplink
        .then(|msg| dispatch_serial(msg, &circuit_breaker, response_rx.clone()))
        .for_each(|r| {
            r.unwrap();
        });

    (downlink_rx, serial_rx)
}

#[tracing::instrument(fields(msg = ?msg), skip(cb, responses))]
pub async fn dispatch_serial(
    msg: Message<OpaqueBytes>,
    cb: &impl failsafe::futures::CircuitBreaker,
    mut responses: impl Stream<Item = Message<payload::Ack>> + Unpin,
) -> Result<Message<payload::Ack>, failsafe::Error<eyre::Report>> {
    loop {
        let result = cb
            .call({
                let responses = &mut responses;
                let header = &msg.header;

                async move {
                    match util::either(
                        responses.find(|target_msg| {
                            let payload = target_msg.payload.as_ref();

                            payload.matches(header)
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
        2,
        failsafe::backoff::equal_jittered(Duration::from_millis(50), Duration::from_millis(2500)),
    );

    failsafe::Config::new().failure_policy(policy).build()
}
