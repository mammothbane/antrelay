use std::path::PathBuf;

use lunarrelay::util::net::Datagram;

type Address = <crate::Socket as Datagram>::Address;

#[derive(Debug, Clone, PartialEq, Eq, structopt::StructOpt)]
pub struct Options {
    #[structopt(long = "downlink")]
    pub downlink_sockets: Vec<Address>,

    #[structopt(long = "uplink")]
    pub uplink_socket: Address,

    #[structopt(short, long)]
    pub serial_port: String,

    #[structopt(short, long, default_value = "115200")]
    pub baud: u32,

    #[structopt(long, default_value = "./libs")]
    pub lib_dir: PathBuf,
}
