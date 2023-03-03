use base64::Engine;
use bytes::Bytes;
use std::{
    io,
    io::Read,
};
use structopt::StructOpt;
use tap::Conv;

use message::{
    self,
    Downlink,
};

#[derive(Debug, Clone, PartialEq, Eq, structopt::StructOpt)]
#[structopt(about = "decode a downlink packet from stdin (default raw binary format)")]
pub struct Mode {
    #[structopt(
        long,
        help = "interpret stdin as hex (can be space and/or newline separated, does not strip 0x)"
    )]
    hex: bool,

    #[structopt(long, help = "interpret stdin as base64")]
    base64: bool,
}

fn main() -> eyre::Result<()> {
    let mode = Mode::from_args();

    let buf = match mode {
        Mode {
            hex: true,
            ..
        } => {
            let mut s = String::new();
            io::stdin().read_to_string(&mut s)?;

            hex::decode(s.trim().replace(&[' ', '\t', '\n', '\r'][..], ""))?
        },
        Mode {
            base64: true,
            ..
        } => {
            let mut s = String::new();
            io::stdin().read_to_string(&mut s)?;

            base64::engine::general_purpose::STANDARD.decode(s.trim())?
        },
        _ => {
            let mut buf = vec![];
            io::stdin().read_to_end(&mut buf)?;

            buf
        },
    };

    let decompressed = util::brotli_decompress(&buf)?;
    let msg = bincode::deserialize::<Downlink>(&decompressed)?;

    match msg {
        Downlink::Log(b) => println!("LOG\n\t{b:?}"),

        Downlink::SerialDownlink(m) => println!("SERIAL DOWNLINK\n\t{m}"),
        Downlink::SerialUplink(m) => println!("SERIAL UPLINK\n\t{m}"),
        Downlink::UplinkInterpreted(m) => println!("UPLINK ECHO\n\t{m}"),

        Downlink::SerialDownlinkRaw(b) => {
            println!("SERIAL DOWNLINK (RAW)\n\t{}", hex::encode(b.conv::<Bytes>()))
        },
        Downlink::SerialUplinkRaw(b) => {
            println!("SERIAL UPLINK (RAW)\n\t{}", hex::encode(b.conv::<Bytes>()))
        },
        Downlink::UplinkMirror(b) => {
            println!("UPLINK ECHO (RAW)\n\t{}", hex::encode(b.conv::<Bytes>()))
        },
    }

    Ok(())
}
