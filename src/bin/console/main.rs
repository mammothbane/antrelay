use std::ffi::OsString;

use async_compat::CompatExt;
use bytes::Bytes;
use packed_struct::PackedStructSlice;
use rustyline_async::ReadlineError;
use structopt::StructOpt;
use tap::Conv;
use tokio::io::{
    AsyncWrite,
    AsyncWriteExt,
};

use message::{
    header::{
        Destination,
        Event,
        Server,
    },
    BytesWrap,
    Downlink,
    Message,
};
use net::{
    DatagramOps,
    DatagramReceiver,
};

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

    #[structopt(name = "ping")]
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

        w.write_all(format!("\n").as_bytes()).await?;
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

        let msg = bincode::deserialize::<Downlink>(&decompressed)?;

        let mut line = match msg {
            Downlink::Log() => "LOG".to_owned().into_bytes(),

            Downlink::SerialDownlinkRaw(b) => bytes_format("SERIAL DOWN (BYTES)", b),
            Downlink::SerialUplinkRaw(b) => bytes_format("SERIAL UP (BYTES)", b),
            Downlink::Direct(b) => bytes_format("DIRECT (BYTES", b),
            Downlink::UplinkMirror(b) => bytes_format("UPLINK (BYTES)", b),

            Downlink::SerialDownlink(m) => msg_format("SERIAL DOWN (MSG)", m),
            Downlink::SerialUplink(m) => msg_format("SERIAL UP (MSG)", m),
            Downlink::UplinkInterpreted(m) => msg_format("UPLINK (MSG)", m),
        };

        line.push(b'\n');

        output.write_all(&line).await?;
    }
}

fn bytes_format(msg: impl AsRef<str>, b: BytesWrap) -> Vec<u8> {
    let mut hex = hex::encode(b.conv::<Bytes>());
    if hex.len() > 60 {
        hex.truncate(60);
        hex.push_str("...<trunc>");
    }

    format!("{}\n\t0x{}\n", msg.as_ref(), hex).as_bytes().to_vec()
}

fn msg_format(msg: impl AsRef<str>, m: Message) -> Vec<u8> {
    format!("{}\n\t{}\n", msg.as_ref(), m).as_bytes().to_vec()
}
