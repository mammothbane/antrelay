use async_compat::CompatExt;
use bytes::{
    BufMut,
    Bytes,
    BytesMut,
};
use packed_struct::PackedStructSlice;
use std::ffi::OsString;

use codec::CobsCodec;
use rustyline_async::ReadlineError;
use structopt::StructOpt;
use tap::Conv;
use tokio::io::{
    AsyncWrite,
    AsyncWriteExt,
};
use tokio_util::codec::Decoder;

use message::{
    header::{
        Destination,
        Disposition,
        Event,
        MessageType,
        Server,
    },
    payload::RelayPacket,
    BytesWrap,
    Message,
    WithCRC,
};
use net::{
    DatagramOps,
    DatagramReceiver,
};
use traceutil::Event as TEvent;

mod options;

pub use options::Options;
use util::brotli_decompress;

#[cfg(windows)]
type Socket = tokio::net::UdpSocket;

#[cfg(unix)]
type Socket = tokio::net::UnixDatagram;

#[derive(structopt::StructOpt)]
#[structopt(setting = structopt::clap::AppSettings::NoBinaryName)]
enum Command {
    PowerSupplied,
    GarageOpenPending,
    RoverStopping,
    RoverMoving,
    DebugPing,
}

#[actix::main]
async fn main() -> eyre::Result<()> {
    let opts: Options = Options::from_args();
    let sock = <Socket as DatagramOps>::connect(&opts.uplink_sock).await?;

    let (mut rl, w) = rustyline_async::Readline::new("> ".to_owned())?;

    tokio::spawn({
        let w = w.clone().compat();

        async move {
            let sock = <Socket as DatagramOps>::bind(&opts.downlink).await.unwrap();
            read_downlink(sock, w).await.unwrap();
        }
    });

    let mut w = w.compat();

    loop {
        w.flush().await?;

        let line = match rl.readline().await {
            Ok(line) => line,

            Err(ReadlineError::Closed)
            | Err(ReadlineError::Eof)
            | Err(ReadlineError::Interrupted) => return Ok(()),

            e @ Err(ReadlineError::IO(_)) => e?,
        };

        let words = match shlex::split(&line) {
            Some(x) => x,
            None => {
                w.write_all(b"failed to split line\n").await?;
                continue;
            },
        };

        let command = match Command::from_iter_safe(words.into_iter().map(OsString::from)) {
            Ok(c) => c,
            Err(e) => {
                w.write_all(&format!("{}", e).as_bytes()).await?;
                continue;
            },
        };

        let ty: Event = match command {
            Command::PowerSupplied => Event::FE_5V_SUP,
            Command::GarageOpenPending => Event::FE_GARAGE_OPEN,
            Command::RoverStopping => Event::FE_ROVER_STOP,
            Command::RoverMoving => Event::FE_ROVER_MOVE,
            #[cfg(debug_assertions)]
            Command::DebugPing => Event::FE_CS_PING,
        };

        let msg =
            message::command(&runtime::params().await, Destination::Frontend, Server::Frontend, ty);

        let pkt = msg.pack_to_vec()?;
        sock.send(&pkt).await?;

        w.write_all(format!("\n=> {:?}\n", ty).as_bytes()).await?;
    }
}

async fn read_downlink<Socket>(
    downlink: Socket,
    mut output: impl AsyncWrite + Unpin,
) -> eyre::Result<()>
where
    Socket: DatagramReceiver + Send + Sync,
    Socket::Error: std::error::Error + Send + Sync + 'static,
{
    let mut buf = vec![0u8; 8192];

    loop {
        output.flush().await?;

        let count = downlink.recv(&mut buf).await?;
        let decompressed = brotli_decompress(&&buf[..count])?;
        let msg = <Message<BytesWrap> as PackedStructSlice>::unpack_from_slice(&decompressed)?;
        let msg = msg.as_ref();

        output.write_all(format!("RECV {}\n", msg.header.display()).as_bytes()).await?;

        match msg.header.ty {
            MessageType {
                server: Server::CentralStation,
                event: Event::CS_PING,
                ..
            } => {
                let inner = msg.payload_into::<WithCRC<RelayPacket>>()?.payload.take();
                let payload = CobsCodec
                    .decode(&mut {
                        let bytes: Bytes = inner.payload.conv::<Bytes>();

                        let mut out = BytesMut::new();
                        out.put(bytes);
                        out
                    })?
                    .unwrap();

                let payload_msg = <Message as PackedStructSlice>::unpack_from_slice(&*payload)?;

                output
                    .write_all(
                        format!(
                            "\tWRAP {:?}\n\t\tWRAP: {}\n",
                            inner.header,
                            payload_msg.as_ref().header.display(),
                        )
                        .as_bytes(),
                    )
                    .await?;
            },

            MessageType {
                server: Server::Frontend,
                event: Event::FE_PING,
                disposition: Disposition::Command,
                ..
            } => {
                let inner = msg.payload_into::<Message>()?.payload.take();
                output
                    .write_all(
                        format!(
                            "\tWRAP {}\n\t\tlen: {}\n",
                            inner.header.display(),
                            inner.payload.as_ref().len(),
                        )
                        .as_bytes(),
                    )
                    .await?;
            },

            MessageType::PONG => {
                let events =
                    bincode::deserialize::<Vec<TEvent>>(&*msg.payload.clone().conv::<Bytes>())?;

                for evt in events {
                    output
                        .write_all(format!("\tLOG {:?}: {:?}\n", evt.ty, evt.args).as_bytes())
                        .await?;
                }
            },

            _ => {
                output.write_all(b"\tpacket unrecognzed\n").await?;
            },
        }
    }
}
