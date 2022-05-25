use async_std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, structopt::StructOpt)]
pub struct Options {
    #[structopt(long = "uplink")]
    pub uplink_sock: PathBuf,

    #[structopt(long = "telemetry")]
    pub telemetry_sock: PathBuf,

    #[structopt(long = "reliable")]
    pub reliable_sock: PathBuf,

    #[structopt(long = "store_and_forward")]
    pub store_and_forward_sock: PathBuf,
}
