use std::path::PathBuf;

type Address = <crate::Socket as net::DatagramOps>::Address;

#[derive(Debug, Clone, PartialEq, Eq, structopt::StructOpt)]
pub struct Options {
    #[structopt(long = "downlink", required = true)]
    pub downlink_addresses: Vec<Address>,

    #[structopt(long = "uplink")]
    pub uplink_address: Address,

    #[structopt(short, long)]
    pub serial_port: String,

    #[structopt(short, long, default_value = "115200")]
    pub baud: u32,

    #[structopt(long, default_value = "./libs")]
    pub lib_dir: PathBuf,
}
