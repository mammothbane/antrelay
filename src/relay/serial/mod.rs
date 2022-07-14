use std::borrow::Borrow;

use backoff::backoff::Backoff;
use futures::{
    AsyncRead,
    AsyncWrite,
};
use smol::{
    channel::Receiver,
    prelude::*,
};
use tap::Pipe;

use crate::{
    message::{
        header::{
            Event,
            Server,
        },
        payload::{
            realtime_status::RealtimeStatus,
            RelayPacket,
        },
        CRCMessage,
        Header,
        OpaqueBytes,
    },
    stream_unwrap,
    util::{
        either,
        PacketEnv,
    },
};

pub mod control;

#[tracing::instrument(level = "debug")]
pub async fn connect_serial(
    path: String,
    baud: u32,
) -> eyre::Result<(impl AsyncRead + Unpin, impl AsyncWrite + Unpin)> {
    let builder = tokio_serial::new(&path, baud);

    let stream =
        async_compat::Compat::new(async { tokio_serial::SerialStream::open(&builder) }).await?;

    let stream = async_compat::Compat::new(stream);
    Ok(smol::io::split(stream))
}

#[tracing::instrument(skip_all, level = "debug")]
pub async fn relay_uplink_to_serial(
    env: impl Borrow<PacketEnv>,
    mut state: control::State,
    uplink: impl Stream<Item = CRCMessage<OpaqueBytes>> + Unpin,
    csq: impl Borrow<CommandSequencer> + Clone,
    request_backoff: impl Backoff + Clone,
    done: Receiver<!>,
) {
    let mut uplink = uplink
        .map(|msg| -> eyre::Result<_> {
            let crc = msg.payload.checksum()?.to_vec();

            Ok((msg, crc))
        })
        .pipe(stream_unwrap!("computing incoming message checksum"))
        .map(|(msg, _crc)| msg);

    let csq = csq.borrow();
    let env = env.borrow();

    loop {
        let result = match either(
            control::state_machine(env, state, &mut uplink, request_backoff.clone(), csq),
            done.recv(),
        )
        .await
        {
            either::Left(result) => result,
            either::Right(_) => return,
        };

        match result {
            Ok(Some(new_state)) => state = new_state,
            Ok(None) => {
                tracing::info!("serial control shutting down");
                break;
            },
            Err(e) => tracing::error!(error = %e, "running serial comms state machine"),
        }
    }
}

#[inline]
pub fn wrap_serial_packets_for_downlink<'e, 'p, 's, 'o>(
    env: impl Borrow<PacketEnv> + 'e,
    packets: impl Stream<Item = Vec<u8>> + 'p,
    status: impl Stream<Item = RealtimeStatus> + 's,
) -> impl Stream<Item = CRCMessage<RelayPacket>> + 'o
where
    'e: 'o,
    'p: 'o,
    's: 'o,
{
    packets.zip(status).map(move |(packet, status)| {
        let mut hdr = Header::downlink(env.borrow(), Event::FE_PING);
        hdr.ty.server = Server::CentralStation;

        CRCMessage::new(hdr, RelayPacket {
            header:  status,
            payload: packet,
        })
    })
}
