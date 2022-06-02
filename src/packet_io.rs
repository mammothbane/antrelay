use async_std::{
    prelude::Stream,
    sync::Mutex,
};
use eyre::WrapErr;
use futures::{
    AsyncBufRead,
    AsyncWrite,
    AsyncWriteExt,
};
use packed_struct::PackedStructSlice;
use smol::{
    io::AsyncBufReadExt,
    stream::StreamExt,
};
use std::sync::Arc;

use crate::{
    message::{
        payload::Ack,
        CRCWrap,
        Message,
        OpaqueBytes,
        UniqueId,
    },
    trace_catch,
};

#[derive(Debug)]
pub struct PacketIO<R, W> {
    r: Mutex<R>,
    w: Mutex<W>,

    pending_acks: dashmap::DashMap<UniqueId, smol::channel::Sender<Message<Ack>>>,
}

impl<R, W> PacketIO<R, W> {
    pub fn new(r: R, w: W) -> Self {
        Self {
            r:            Mutex::new(r),
            w:            Mutex::new(w),
            pending_acks: dashmap::DashMap::new(),
        }
    }

    #[tracing::instrument(skip_all, err, level = "debug")]
    pub async fn request<T>(&self, msg: &Message<T>) -> eyre::Result<ReqHandle<'_, R, W>>
    where
        W: AsyncWrite + Unpin,
        T: PackedStructSlice,
    {
        let (tx, rx) = smol::channel::bounded(1);

        let message_id = msg.header.unique_id();

        let old_sender = self.pending_acks.insert(message_id, tx);
        debug_assert!(old_sender.is_none());

        {
            let mut w = self.w.lock().await;
            w.write_all(&msg.pack_to_vec()?).await?;
        }

        Ok(ReqHandle {
            parent:   self,
            id:       message_id,
            receiver: rx,
        })
    }

    #[tracing::instrument(skip(self), level = "debug")]
    pub async fn read_packets(self: Arc<Self>, sentinel: u8) -> impl Stream<Item = Vec<u8>>
    where
        R: AsyncBufRead + Unpin + 'static,
        W: 'static,
    {
        smol::stream::unfold((Vec::with_capacity(8192), self), move |(mut buf, s)| {
            buf.truncate(0);

            Box::pin(async move {
                tracing::trace!("waiting for serial message");

                let count: usize = {
                    let mut r = s.r.lock().await;
                    let result = r.read_until(sentinel, &mut buf).await;
                    trace_catch!(result, "receiving serial message");

                    result.ok()?
                };

                let bytes = &mut buf[..count];

                let data = {
                    cfg_if::cfg_if! {
                        if #[cfg(feature = "serial_cobs")] {
                            let result = cobs::decode_in_place_with_sentinel(bytes, sentinel).map_err(|()| eyre::eyre!("cobs decode failed"));
                            trace_catch!(result, "cobs decode");

                            &bytes[..result.ok()?]
                        } else {
                            bytes
                        }
                    }
                };

                trace_catch!(s.handle_ack(&data), "handling message as ack");

                Some((data.to_vec(), (buf, s)))
            })
        })
        .fuse()
    }

    fn handle_ack(&self, data: &[u8]) -> eyre::Result<()> {
        let msg = <Message<OpaqueBytes> as PackedStructSlice>::unpack_from_slice(data)?;
        if !msg.header.ty.ack {
            return Ok(());
        }

        let ack = msg.payload_into::<CRCWrap<Ack>>().wrap_err("parsing ack message")?;
        let message_id = ack.payload.as_ref().unique_id();

        match self.pending_acks.remove(&message_id) {
            Some((_, tx)) => {
                debug_assert_eq!(tx.len(), 0);

                tx.try_send(ack)?;
            },
            None => {
                tracing::warn!(id = ?message_id, "no registered handler for message");
            },
        }

        Ok(())
    }
}

pub struct ReqHandle<'a, R, W> {
    parent:   &'a PacketIO<R, W>,
    id:       UniqueId,
    receiver: smol::channel::Receiver<Message<Ack>>,
}

impl<'a, R, W> ReqHandle<'a, R, W> {
    #[inline]
    pub async fn wait(&self) -> Result<Message<Ack>, smol::channel::RecvError> {
        self.receiver.recv().await
    }
}

impl<'a, R, W> Drop for ReqHandle<'a, R, W> {
    #[inline]
    fn drop(&mut self) {
        self.parent.pending_acks.remove(&self.id);
    }
}
