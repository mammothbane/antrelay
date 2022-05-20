#[derive(Debug, Clone, PartialEq, Eq, structopt::StructOpt)]
pub struct Options {
    #[structopt(long = "uplink")]
    pub uplink_sock: async_std::path::PathBuf,

    #[structopt(long = "telemetry")]
    pub telemetry_sock: async_std::path::PathBuf,

    #[structopt(long = "reliable")]
    pub reliable_sock: async_std::path::PathBuf,

    #[structopt(long = "store_and_forward")]
    pub store_and_forward_sock: async_std::path::PathBuf,
}
