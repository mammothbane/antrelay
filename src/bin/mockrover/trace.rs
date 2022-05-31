use std::str::FromStr;

use tracing_subscriber::{
    filter::EnvFilter,
    fmt::format::FmtSpan,
};

use lunarrelay::bootstrap;

pub const DEFAULT_LEVEL_STR: &str = {
    cfg_if::cfg_if! {
        if #[cfg(not(debug_assertions))] {
            "warn,lunarrelay=info,mockrover=info"
        } else {
            "info,lunarrelay=debug,mockrover=debug"
        }
    }
};

pub fn init() {
    let level_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::from_str(DEFAULT_LEVEL_STR).expect("parsing envfilter default string")
    });

    bootstrap!("enabling tracing with filter directive: {}", level_filter);

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_span_events(FmtSpan::CLOSE)
        .with_env_filter(level_filter)
        .pretty()
        .init();
}
