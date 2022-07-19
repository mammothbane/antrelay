type Address = <crate::Socket as net::DatagramOps>::Address;

#[derive(Debug, Clone, PartialEq, Eq, structopt::StructOpt)]
pub struct Options {
    #[structopt(long = "uplink", required = true)]
    pub uplink_sock: Address,

    #[structopt(long, required = true)]
    pub downlink: Address,
}
