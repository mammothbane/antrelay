use std::{
    borrow::Borrow,
    time::Duration,
};

use backoff::backoff::Backoff;
use smol::prelude::*;

use crate::{
    io::CommandSequencer,
    message::{
        header::Event,
        CRCMessage,
        Header,
        OpaqueBytes,
    },
    util::{
        self,
        send,
        PacketEnv,
    },
};

#[tracing::instrument(skip(env, uplink_messages, backoff, csq), ret, err(Display), level = "debug")]
pub async fn state_machine(
    env: impl Borrow<PacketEnv>,
    state: State,
    mut uplink_messages: impl Stream<Item = CRCMessage<OpaqueBytes>> + Unpin,
    backoff: impl Backoff + Clone,
    csq: impl Borrow<CommandSequencer>,
) -> eyre::Result<Option<State>> {
    tracing::debug!(?state, "state machine start");


    Ok(result)
}
