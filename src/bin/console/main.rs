use std::ffi::OsString;

use async_compat::CompatExt;
use packed_struct::PackedStructSlice;
use rustyline_async::ReadlineError;
use structopt::StructOpt;
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
    Downlink,
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

        let msg = bincode::deserialize::<Downlink>(&decompressed)?;

        let line = match msg {
            Downlink::Log() => "LOG".to_owned(),
            Downlink::SerialDownlinkRaw(_b) => "SERIAL DOWN RAW".to_owned(),
            Downlink::SerialDownlink(_m) => "SERIAL DOWN".to_owned(),
            Downlink::SerialUplinkRaw(_b) => "SERIAL UP RAW".to_owned(),
            Downlink::SerialUplink(_m) => "SERIAL UP".to_owned(),
            Downlink::Direct(_b) => "DIRECT".to_owned(),
            Downlink::UplinkMirror(_b) => "UPLINK MIRROR".to_owned(),
            Downlink::UplinkInterpreted(_m) => "UPLINK".to_owned(),
        };

        output.write_all(format!("{}\n", line).as_bytes()).await?;
    }
}
