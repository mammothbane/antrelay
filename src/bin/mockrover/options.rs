use lunarrelay::util::net::Datagram;

type Address = <crate::Socket as Datagram>::Address;

#[derive(Debug, Clone, PartialEq, Eq, structopt::StructOpt)]
pub struct Options {
    #[structopt(long = "uplink")]
    pub uplink_sock: Address,

    #[structopt(long = "downlink", required = true)]
    pub downlink: Vec<Address>,

    #[structopt(long = "serial_port")]
    pub serial_port: String,
}
