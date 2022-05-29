use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use async_std::{
    prelude::Stream,
    sync::{
        Arc,
        Mutex,
    },
};
use eyre::WrapErr;
use futures::{
    AsyncBufRead,
    AsyncWrite,
    AsyncWriteExt,
};
use packed_struct::PackedStructSlice;
use smol::{
    future::FutureExt,
    io::AsyncBufReadExt,
    stream::StreamExt,
};

use lunarrelay::{
    message::{
        payload::Ack,
        Message,
        OpaqueBytes,
    },
    util,
};

use crate::relay::MessageId;

#[derive(Debug)]
pub struct PacketIO<R, W> {
    r: RefCell<R>,
    w: Mutex<W>,

    shutdown: smol::channel::Receiver<!>,

    pending_acks: dashmap::DashMap<MessageId, smol::channel::Sender<Arc<Message<OpaqueBytes>>>>,
}

impl<R, W> PacketIO<R, W> {
    pub fn new(r: R, w: W, shutdown: smol::channel::Receiver<!>) -> Self {
        Self {
            r: RefCell::new(r),
            w: Mutex::new(w),
            shutdown,
            pending_acks: dashmap::DashMap::new(),
        }
    }

    pub async fn request<T, U>(&self, msg: Message<T>) -> eyre::Result<Guard<'_, R, W>>
    where
        W: AsyncWrite + Unpin,
        T: PackedStructSlice,
    {
        let (tx, rx) = smol::channel::bounded(1);

        let message_id = MessageId(msg.header.timestamp, msg.header.seq);

        let old_sender = self.pending_acks.insert(message_id, tx);
        debug_assert!(old_sender.is_none());

        let written = async {
            let mut w = self.w.lock().await;
            w.write_all(&msg.pack_to_vec()?).await?;

            Ok(())
        };

        match util::either(written, self.shutdown.recv()).await {
            either::Right(_) => return Err(eyre::eyre!("shutdown")),
            either::Left(_) => {},
        }

        Ok(Guard {
            parent:   self,
            id:       message_id,
            receiver: rx,
        })
    }

    pub fn read_messages(&self, sentinel: u8) -> impl Stream<Item = Arc<Message<OpaqueBytes>>>
    where
        R: AsyncBufRead + Unpin,
    {
        let mut buf = vec![0u8; 8192];

        smol::stream::try_unfold(self.r.borrow_mut(), |mut r| {
            let buf = &mut buf;

            async move {
                let count =
                    match util::either(r.read_until(sentinel, buf), self.shutdown.recv()).await {
                        either::Right(_) => return Ok(None),
                        either::Left(count) => count,
                    };

                let bytes = &mut buf[..count];

                let data = {
                    cfg_if::cfg_if! {
                        if #[cfg(feature = "serial_cobs")] {
                            cobs::decode_in_place_with_sentinel(bytes, sentinel)
                        } else {
                            bytes
                        }
                    }
                };

                let packet = <Message<OpaqueBytes> as PackedStructSlice>::unpack_from_slice(data)?;
                Ok(Some((packet, r))) as eyre::Result<Option<(Message<OpaqueBytes>, _)>>
            }
        })
        .filter_map(|msg_result| {
            lunarrelay::trace_catch!(msg_result, "reading message over serial");
            msg_result.ok()
        })
        .map(Arc::new)
        .map(|msg| {
            if !msg.header.ty.ack {
                return msg;
            }

            let Ok(ack): Result<Message<Ack>, _> = msg.try_into().wrap_err("parsing ack message") else {
                return msg;
            };

            let payload = ack.payload.as_ref();

            let message_id = MessageId(payload.timestamp, payload.seq);

            match self.pending_acks.remove(&message_id) {
                Some((_, tx)) => {
                    debug_assert_eq!(tx.len(), 0);

                    let send_result = tx.try_send(msg.clone());
                    lunarrelay::trace_catch!(send_result, "channel was full");
                },
                None => {
                    tracing::warn!(id = ?message_id, "no registered handler for message")
                },
            }

            msg
        })
    }
}

pub struct Guard<'a, R, W> {
    parent:   &'a PacketIO<R, W>,
    id:       MessageId,
    receiver: smol::channel::Receiver<Arc<Message<OpaqueBytes>>>,
}

impl<'a, R, W> Guard<'a, R, W> {
    #[inline]
    pub async fn wait(&self) -> Result<Arc<Message<OpaqueBytes>>, smol::channel::RecvError> {
        self.receiver.recv().await
    }
}

impl<'a, R, W> Drop for Guard<'a, R, W> {
    #[inline]
    fn drop(&mut self) {
        self.parent.pending_acks.remove(&self.id);
    }
}
