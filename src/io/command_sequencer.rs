use std::sync::Arc;

use dashmap::DashMap;
use eyre::WrapErr;
use packed_struct::PackedStructSlice;
use smol::{
    channel::Sender,
    prelude::*,
};

use crate::message::{
    header::Disposition,
    payload::Ack,
    CRCWrap,
    Message,
    OpaqueBytes,
    UniqueId,
};

type AckMap = DashMap<UniqueId, Sender<Message<Ack>>>;

pub struct CommandSequencer {
    pending_acks: Arc<AckMap>,
    tx:           Sender<Vec<u8>>,
}

impl CommandSequencer {
    pub fn new(
        serial_read: impl Stream<Item = Vec<u8>>,
    ) -> (Self, impl Stream<Item = eyre::Result<()>>, impl Stream<Item = Vec<u8>>) {
        let (tx, rx) = smol::channel::unbounded();

        let ret = CommandSequencer {
            pending_acks: Arc::new(DashMap::new()),
            tx,
        };

        let pending_acks = ret.pending_acks.clone();
        let response_packets =
            serial_read.map(move |pkt| Self::handle_maybe_ack_packet(&pkt, pending_acks.as_ref()));

        (ret, response_packets, rx)
    }

    #[tracing::instrument(skip(self, msg), level = "debug", err)]
    pub async fn submit(&self, msg: &Message<OpaqueBytes>) -> eyre::Result<Message<Ack>> {
        let (tx, rx) = smol::channel::bounded(1);
        let message_id = msg.header.unique_id();

        let _clean_on_drop = CleanOnDrop {
            pending_acks: &self.pending_acks,
            id:           message_id,
        };

        let old_sender = self.pending_acks.insert(message_id, tx);
        debug_assert!(old_sender.is_none());

        self.tx.send(msg.pack_to_vec()?).await?;
        let ret = rx.recv().await?;
        tracing::debug!("post-receive");
        Ok(ret)
    }

    #[tracing::instrument(fields(packet = ?packet), skip(acks), err, level = "debug")]
    fn handle_maybe_ack_packet(
        packet: &[u8],
        acks: &DashMap<UniqueId, Sender<Message<Ack>>>,
    ) -> eyre::Result<()> {
        let msg = <Message<OpaqueBytes> as PackedStructSlice>::unpack_from_slice(packet)?;
        if msg.header.ty.disposition != Disposition::Response {
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
