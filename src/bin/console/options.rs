type Address = <crate::Socket as net::DatagramOps>::Address;

#[derive(Debug, Clone, PartialEq, Eq, structopt::StructOpt)]
pub struct Options {
    #[structopt(long, required = true)]
    #[cfg_attr(unix, structopt(help = "path to uplink socket"))]
    #[cfg_attr(
        windows,
        structopt(
            help = "uplink socket addr, e.g. 127.0.0.1:3000 (must be an IP: no hostname resolution available)"
        )
    )]
    pub uplink: Address,

    #[structopt(long, required = true)]
    #[cfg_attr(unix, structopt(help = "path to downlink socket"))]
    #[cfg_attr(
        windows,
        structopt(
            help = "downlink socket addr, e.g. 127.0.0.1:3000 (must be an IP: no hostname resolution available)"
        )
    )]
    pub downlink: Address,
}
