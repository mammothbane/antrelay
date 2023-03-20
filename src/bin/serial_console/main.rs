#![feature(try_blocks)]

use std::{
    ffi::OsString,
    sync::{
        atomic,
        atomic::Ordering,
        Arc,
    },
};

use async_compat::CompatExt;
use bytes::{
    Bytes,
    BytesMut,
};
use codec::CobsCodec;
use futures::{
    SinkExt,
    Stream,
    StreamExt,
};
use message::header::{
    Destination,
    Disposition,
    Event,
    MessageType,
};
use packed_struct::PackedStructSlice;
use rustyline_async::ReadlineError;
use structopt::StructOpt;
use tokio::{
    io::{
        AsyncWrite,
        AsyncWriteExt,
        WriteHalf,
    },
    sync::Mutex,
};
use tokio_serial::SerialStream;
use tokio_util::codec::{
    Decoder,
    FramedRead,
    FramedWrite,
};

use message::{
    source_info::Info,
    BytesWrap,
    Header,
    HeaderPacket,
    Message,
    MissionEpoch,
    SourceInfo,
    StandardCRC,
};

mod options;

pub use options::Options;

static AUTOACK: atomic::AtomicBool = atomic::AtomicBool::new(true);

#[derive(structopt::StructOpt)]
#[structopt(setting = structopt::clap::AppSettings::NoBinaryName)]
enum Command {
    Literal {
        #[structopt(required = true)]
        value: String,
    },

    CobsLiteral {
        #[structopt(required = true)]
        value: String,
    },

    #[structopt(name = "autoack")]
    AutoAck,

    SendSpam,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let opts: Options = Options::from_args();

    let builder = tokio_serial::new(opts.port, opts.baud);
    let stream = tokio_serial::SerialStream::open(&builder)?;

    let (reader, writer) = tokio::io::split(stream);

    let framed_read = FramedRead::new(reader, CobsCodec);
    let framed_write = FramedWrite::new(writer, CobsCodec);
    let framed_write = Arc::new(Mutex::new(framed_write));

    let (mut rl, w) = rustyline_async::Readline::new("> ".to_owned())?;

    tokio::spawn({
        let w = w.clone().compat();
        let framed_write = framed_write.clone();

        async move {
            read_downlink(framed_read, w, framed_write).await.unwrap();
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
                w.write_all(format!("command error: {e}\n").as_bytes()).await?;
                continue;
            },
        };

        match command {
            Command::AutoAck => {
                let old = AUTOACK.fetch_xor(true, Ordering::SeqCst);
                w.write_all(format!("toggling autoack {old} -> {}", !old).as_bytes()).await?;
            },

            Command::CobsLiteral {
                value,
            } => {
                let val = match hex::decode(value) {
                    Ok(val) => val,
                    Err(e) => {
                        w.write_all(format!("error: invalid argument: {e}\n").as_bytes()).await?;
                        continue;
                    },
                };

                let b: eyre::Result<Bytes> = try {
                    let mut bytes = BytesMut::from(&val[..]);
                    CobsCodec
                        .decode_eof(&mut bytes)?
                        .ok_or(eyre::eyre!("could not read cobs message"))?
                };

                match b {
                    Err(e) => {
                        w.write_all(format!("error parsing cobs data: {e}\n").as_bytes()).await?;
                        continue;
                    },
                    Ok(b) => {
                        let mut wr = framed_write.lock().await;
                        wr.send(b.to_vec()).await?
                    },
                }
            },

            Command::Literal {
                value,
            } => {
                let val = match hex::decode(value) {
                    Ok(val) => val,
                    Err(e) => {
                        w.write_all(format!("error: invalid argument: {e}\n").as_bytes()).await?;
                        continue;
                    },
                };

                let mut wr = framed_write.lock().await;
                wr.send(val).await?;
            },

            Command::SendSpam => {
                let msg = Message::<_, StandardCRC>::new(HeaderPacket {
                    header:  HeaderPacket {
                        header:  Header {
                            magic:     Default::default(),
                            timestamp: MissionEpoch::new(0),
                            ty:        MessageType {
                                disposition: Disposition::Ack,
                                event:       Event::AntPing,
                                invalid:     false,
                            },

                            seq:         0,
                            destination: Destination::Frontend,
                        },
                        payload: SourceInfo::Empty,
                    },
                    payload: BytesWrap::default(),
                });

                let bytes = msg.pack_to_vec()?;

                for _ in 0..10 {
                    let mut wr = framed_write.lock().await;
                    wr.send(&bytes).await?;
                }
            },
        }

        w.write_all("\n".as_bytes()).await?;
    }
}

async fn read_downlink(
    mut downlink: impl Stream<Item = Result<Bytes, codec::cobs::Error>> + Unpin,
    mut output: impl AsyncWrite + Unpin,
    uplink: Arc<Mutex<FramedWrite<WriteHalf<SerialStream>, CobsCodec>>>,
) -> eyre::Result<()> {
    loop {
        output.flush().await?;

        let packet = match downlink.next().await {
            Some(x) => x?,
            None => return Ok(()),
        };

        let mut message = <Message as PackedStructSlice>::unpack_from_slice(&packet)?;

        output.write_all(format!("UP PACKET\n\t{}\n", hex::encode(&*packet)).as_bytes()).await?;
        output.write_all(format!("UP MESSAGE\n\t{message}\n").as_bytes()).await?;

        if !AUTOACK.load(Ordering::SeqCst) {
            continue;
        }

        message.header.payload = SourceInfo::Info(Info {
            header:   message.header.header,
            checksum: message.checksum()?[0],
        });

        message.header.header.ty.disposition = Disposition::Ack;
        message.payload = BytesWrap::default();

        let bytes = message.pack_to_vec()?;

        let mut uplink = uplink.lock().await;
        uplink.send(&bytes).await?;
    }
}
