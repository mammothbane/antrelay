use std::sync::Arc;

use dashmap::DashMap;
use eyre::WrapErr;
use smol::{
    channel::Sender,
    prelude::*,
};

use crate::message::{
    header::Disposition,
    payload::Ack,
    CRCMessage,
    CRCWrap,
    OpaqueBytes,
    UniqueId,
};

type AckMap = DashMap<UniqueId, Sender<CRCMessage<Ack>>>;

pub struct CommandSequencer {
    pending_acks: Arc<AckMap>,
    uplink_q:     Sender<CRCMessage<OpaqueBytes>>,
}

impl CommandSequencer {
    pub fn new(
        downlink: impl Stream<Item = CRCMessage<OpaqueBytes>>,
    ) -> (Self, impl Stream<Item = eyre::Result<()>>, impl Stream<Item = CRCMessage<OpaqueBytes>>)
    {
        let (uplink_tx, uplink_rx) = smol::channel::unbounded();

        let ret = CommandSequencer {
            pending_acks: Arc::new(DashMap::new()),
            uplink_q:     uplink_tx,
        };

        let pending_acks = ret.pending_acks.clone();
        let downlink_packets =
            downlink.map(move |pkt| Self::handle_maybe_ack_packet(&pkt, pending_acks.as_ref()));

        (ret, downlink_packets, uplink_rx)
    }

    #[tracing::instrument(skip(self, msg), level = "debug", err)]
    pub async fn submit(&self, msg: CRCMessage<OpaqueBytes>) -> eyre::Result<CRCMessage<Ack>> {
        let (tx, rx) = smol::channel::bounded(1);
        let message_id = msg.header.unique_id();

        let _clean_on_drop = CleanOnDrop {
            pending_acks: &self.pending_acks,
            id:           message_id,
        };

        let old_sender = self.pending_acks.insert(message_id, tx);
        debug_assert!(old_sender.is_none());

        self.uplink_q.send(msg).await?;
        let ret = rx.recv().await?;
        Ok(ret)
    }

    #[tracing::instrument(fields(msg = ?msg), skip(acks), err, level = "debug")]
    fn handle_maybe_ack_packet(
        msg: &CRCMessage<OpaqueBytes>,
        acks: &DashMap<UniqueId, Sender<CRCMessage<Ack>>>,
    ) -> eyre::Result<()> {
        if msg.header.ty.disposition != Disposition::Ack {
            return Ok(());
        }

        let ack = msg.payload_into::<CRCWrap<Ack>>()?;
        let message_id = ack.payload.as_ref().unique_id();

        match acks.remove(&message_id) {
            Some((_, tx)) => {
                debug_assert_eq!(tx.len(), 0);

                tx.try_send(ack).wrap_err("sending ack reply to submitted message")?;
            },
            None => {
                tracing::warn!(id = ?message_id, "no registered handler for message");
            },
        }

        Ok(())
    }
}

struct CleanOnDrop<'a> {
    pending_acks: &'a AckMap,
    id:           UniqueId,
}

impl<'a> Drop for CleanOnDrop<'a> {
    #[inline]
    fn drop(&mut self) {
        self.pending_acks.remove(&self.id);
    }
}
