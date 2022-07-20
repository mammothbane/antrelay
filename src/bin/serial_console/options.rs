#[derive(Debug, Clone, PartialEq, Eq, structopt::StructOpt)]
pub struct Options {
    #[structopt(long = "serial_port", required = true)]
    pub port: String,

    #[structopt(long = "baud", default_value = "115200")]
    pub baud: u32,
}
