use dashmap::DashMap;
use eyre::WrapErr;
use packed_struct::PackedStructSlice;
use smol::{
    channel::Sender,
    lock::Mutex,
    prelude::*,
};
use std::sync::Arc;

use crate::message::{
    payload::Ack,
    CRCWrap,
    Message,
    OpaqueBytes,
    RawMessage,
    UniqueId,
};

type AckMap = DashMap<UniqueId, Sender<Message<Ack>>>;

pub struct CommandSequencer<W> {
    writer:       Mutex<W>,
    pending_acks: Arc<AckMap>,
}

impl<W> CommandSequencer<W> {
    pub fn new(
        r: impl Stream<Item = Vec<u8>>,
        w: W,
    ) -> (Self, impl Stream<Item = eyre::Result<()>>) {
        let ret = CommandSequencer {
            writer:       Mutex::new(w),
            pending_acks: Arc::new(DashMap::new()),
        };

        let pending_acks = ret.pending_acks.clone();
        let stream = r.map(move |pkt| Self::handle_maybe_ack_packet(&pkt, pending_acks.as_ref()));

        (ret, stream)
    }

    pub fn submit<'a>(
        &'a self,
        msg: RawMessage<OpaqueBytes>,
    ) -> impl Future<Output = eyre::Result<Message<Ack>>> + 'a
    where
        W: AsyncWrite + Unpin,
    {
        let (tx, rx) = smol::channel::bounded(1);

        let message_id = msg.header.unique_id();

        let old_sender = self.pending_acks.insert(message_id, tx);
        debug_assert!(old_sender.is_none());

        let handle = ReqHandle {
            pending_acks: &self.pending_acks,
            id:           message_id,
            receiver:     rx,
        };

        async move {
            {
                let mut w = self.writer.lock().await;
                w.write_all(&msg.pack_to_vec()?).await?;
            }

            let ret = handle.wait().await?;
            Ok(ret)
        }
    }

    fn handle_maybe_ack_packet(
        packet: &[u8],
        acks: &DashMap<UniqueId, Sender<Message<Ack>>>,
    ) -> eyre::Result<()> {
        let msg = <Message<OpaqueBytes> as PackedStructSlice>::unpack_from_slice(packet)?;
        if !msg.header.ty.ack {
            return Ok(());
        }

        let ack = msg.payload_into::<CRCWrap<Ack>>().wrap_err("parsing message body as ack")?;
        let message_id = ack.payload.as_ref().unique_id();

        match acks.remove(&message_id) {
            Some((_, tx)) => {
                debug_assert_eq!(tx.len(), 0);

                tx.try_send(ack).wrap_err("replying ")?;
            },
            None => {
                tracing::warn!(id = ?message_id, "no registered handler for message");
            },
        }

        Ok(())
    }
}

pub struct ReqHandle<'a> {
    pending_acks: &'a AckMap,
    id:           UniqueId,
    receiver:     smol::channel::Receiver<Message<Ack>>,
}

impl<'a> ReqHandle<'a> {
    #[inline]
    pub async fn wait(&self) -> Result<Message<Ack>, smol::channel::RecvError> {
        self.receiver.recv().await
    }
}

impl<'a> Drop for ReqHandle<'a> {
    #[inline]
    fn drop(&mut self) {
        self.pending_acks.remove(&self.id);
    }
}
