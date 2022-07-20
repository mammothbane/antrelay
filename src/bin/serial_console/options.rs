#[derive(Debug, Clone, PartialEq, Eq, structopt::StructOpt)]
pub struct Options {
    #[structopt(long = "serial_port", required = true)]
    #[cfg_attr(unix, structopt(help = "path to serial port"))]
    #[cfg_attr(windows, structopt(help = "serial port (e.g. COM8)"))]
    pub port: String,

    #[structopt(long = "baud", default_value = "115200", help = "serial baud rate (optional)")]
    pub baud: u32,
}
