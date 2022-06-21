use std::ffi::OsString;

use rustyline_async::ReadlineError;
use smol::io::{
    AsyncWrite,
    AsyncWriteExt,
};
use structopt::StructOpt;

use antrelay::{
    message::{
        header::{
            Disposition,
            Event,
            RequestMeta,
            Server,
        },
        payload::RelayPacket,
        CRCMessage,
        CRCWrap,
        Header,
        OpaqueBytes,
        StandardCRC,
    },
    net::{
        DatagramOps,
        DatagramReceiver,
    },
    tracing::Event as TEvent,
    util::{
        PacketEnv,
        DEFAULT_DOWNLINK_CODEC,
        DEFAULT_SERIAL_CODEC,
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
    smol::spawn({
        let w = w.clone();
        async move {
            let sock = <Socket as DatagramOps>::bind(&opts.downlink).await.unwrap();
            read_downlink(sock, w).await.unwrap();
        }
    })
    .detach();

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
        };

        let msg =
            CRCMessage::<Vec<u8>, StandardCRC>::new(Header::fe_command(&env, ty), vec![0u8; 0]);
        let pkt = (DEFAULT_UPLINK_CODEC.encode)(msg)?;
        sock.send(&pkt).await?;

        w.write_all(format!("=> {:?}\n", ty).as_bytes()).await?;
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
        let pkt = &buf[..count];

        let msg = (DEFAULT_DOWNLINK_CODEC.decode)(pkt.to_vec())?;

        output.write_all(format!("RECV {}\n", msg.header.display()).as_bytes()).await?;

        match msg.header.ty {
            RequestMeta {
                server: Server::CentralStation,
                event: Event::CS_PING,
                ..
            } => {
                let inner = msg.payload_into::<CRCWrap<RelayPacket>>()?.payload.take();
                let payload = (DEFAULT_SERIAL_CODEC.decode)(inner.payload)?;

                output
                    .write_all(
                        format!(
                            "\tWRAP {:?}\n\t\tWRAP: {}\n",
                            inner.header,
                            payload.header.display(),
                        )
                        .as_bytes(),
                    )
                    .await?;
            },

            RequestMeta {
                server: Server::Frontend,
                event: Event::FE_PING,
                disposition: Disposition::Command,
                ..
            } => {
                let inner = msg.payload_into::<CRCWrap<CRCMessage<OpaqueBytes>>>()?.payload.take();
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

            RequestMeta::PONG => {
                let events = bincode::deserialize::<Vec<TEvent>>(msg.payload.payload_bytes()?)?;

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
