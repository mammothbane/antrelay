use async_std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, structopt::StructOpt)]
pub struct Options {
    #[structopt(long = "uplink", required_unless = "disable_unix_sockets", default_value = ".")]
    pub uplink_sock: PathBuf,

    #[structopt(long = "telemetry", required_unless = "disable_unix_sockets", default_value = ".")]
    pub telemetry_sock: PathBuf,

    #[structopt(long = "reliable", required_unless = "disable_unix_sockets", default_value = ".")]
    pub reliable_sock: PathBuf,

    #[structopt(
        long = "store_and_forward",
        required_unless = "disable_unix_sockets",
        default_value = "."
    )]
    pub store_and_forward_sock: PathBuf,

    #[structopt(long)]
    pub disable_unix_sockets: bool,

    #[structopt(short, long)]
    pub serial_port: String,

    #[structopt(short, long, default_value = "115200")]
    pub baud: u32,
}
