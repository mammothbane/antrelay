use std::ffi::OsString;

use rustyline_async::ReadlineError;
use smol::io::AsyncWriteExt;
use structopt::StructOpt;

use antrelay::{
    message::{
        header::Conversation,
        CRCMessage,
        Header,
        StandardCRC,
    },
    net::DatagramOps,
    util::{
        PacketEnv,
        DEFAULT_UPLINK_CODEC,
    },
};

mod options;

pub use options::Options;

#[cfg(windows)]
type Socket = smol::net::UdpSocket;

#[cfg(unix)]
type Socket = smol::net::unix::UnixDatagram;

#[inline]
fn main() -> eyre::Result<()> {
    smol::block_on(_main())
}

#[derive(structopt::StructOpt)]
#[structopt(setting = structopt::clap::AppSettings::NoBinaryName)]
enum Command {
    PowerSupplied,
    GarageOpenPending,
    RoverStopping,
    RoverMoving,
}

async fn _main() -> eyre::Result<()> {
    let opts: Options = Options::from_args();
    let sock = <Socket as DatagramOps>::connect(&opts.uplink_sock).await?;
    let env = PacketEnv::default();

    let (mut rl, mut w) = rustyline_async::Readline::new("> ".to_owned())?;

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

        let ty: Conversation = match command {
            Command::PowerSupplied => Conversation::PowerSupplied,
            Command::GarageOpenPending => Conversation::GarageOpenPending,
            Command::RoverStopping => Conversation::RoverHalting,
            Command::RoverMoving => Conversation::RoverWillTurn,
        };

        let msg =
            CRCMessage::<Vec<u8>, StandardCRC>::new(Header::fe_command(&env, ty), vec![0u8; 0]);
        let pkt = (DEFAULT_UPLINK_CODEC.encode)(msg)?;
        sock.send(&pkt).await?;

        w.write_all(b"sent\n").await?;
    }
}
