#[derive(Debug, Clone, PartialEq, Eq, structopt::StructOpt)]
pub struct Options {
    #[structopt(long = "downlink", required = true)]
    #[cfg_attr(unix, structopt(help = "paths to downlink sockets (as many as desired)"))]
    #[cfg_attr(
        windows,
        structopt(
            help = "downlink socket addrs, e.g. 127.0.0.1:3000 (must be IPs: no hostname resolution available)"
        )
    )]
    pub downlink_addresses: Vec<antrelay::Address>,

    #[structopt(long = "uplink")]
    #[cfg_attr(unix, structopt(help = "path to uplink socket"))]
    #[cfg_attr(
        windows,
        structopt(
            help = "uplink socket addr, e.g. 127.0.0.1:3000 (must be an IP: no hostname resolution available)"
        )
    )]
    pub uplink_address: antrelay::Address,

    #[structopt(short, long)]
    #[cfg_attr(unix, structopt(help = "path to serial port"))]
    #[cfg_attr(windows, structopt(help = "serial port (e.g. COM8)"))]
    pub serial_port: String,

    #[structopt(short, long, default_value = "115200", help = "serial baud rate (optional)")]
    pub baud: u32,

    #[structopt(long, help = "pretty log output")]
    pub pretty: bool,
}
