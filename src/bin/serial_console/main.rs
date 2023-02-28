#![feature(try_blocks)]

use std::ffi::OsString;

use async_compat::CompatExt;
use bytes::{
    Bytes,
    BytesMut,
};
use codec::CobsCodec;
use futures::{
    prelude::*,
    Stream,
    StreamExt,
};
use packed_struct::PackedStructSlice;
use rustyline_async::ReadlineError;
use structopt::StructOpt;
use tokio::io::{
    AsyncWrite,
    AsyncWriteExt,
};
use tokio_util::codec::{
    Decoder,
    FramedRead,
    FramedWrite,
};

use message::Message;

mod options;

pub use options::Options;

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
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let opts: Options = Options::from_args();

    let builder = tokio_serial::new(opts.port, opts.baud);
    let stream = tokio_serial::SerialStream::open(&builder)?;

    let (reader, writer) = tokio::io::split(stream);

    let framed_read = FramedRead::new(reader, CobsCodec);
    let mut framed_write = FramedWrite::new(writer, CobsCodec);

    let (mut rl, w) = rustyline_async::Readline::new("> ".to_owned())?;

    tokio::spawn({
        let w = w.clone().compat();

        async move {
            read_downlink(framed_read, w).await.unwrap();
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
                w.write_all("toggling autoack".as_bytes()).await?;
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
                    Ok(b) => framed_write.send(b.to_vec()).await?,
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

                framed_write.send(val).await?
            },
        }

        w.write_all("\n".as_bytes()).await?;
    }
}

async fn read_downlink(
    mut downlink: impl Stream<Item = Result<Bytes, codec::cobs::Error>> + Unpin,
    mut output: impl AsyncWrite + Unpin,
) -> eyre::Result<()> {
    loop {
        output.flush().await?;

        let packet = match downlink.next().await {
            Some(x) => x?,
            None => return Ok(()),
        };
        output.write_all(format!("UP PACKET\n\t{}\n", hex::encode(&*packet)).as_bytes()).await?;

        let message = <Message as PackedStructSlice>::unpack_from_slice(&packet)?;
        output.write_all(format!("UP MESSAGE\n\t{message}\n").as_bytes()).await?;
    }
}
