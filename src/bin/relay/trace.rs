use std::str::FromStr;

use smol::stream::Stream;
use tracing_subscriber::{
    fmt::format::FmtSpan,
    prelude::*,
    EnvFilter,
};

use antrelay::bootstrap;

pub fn init() -> eyre::Result<impl Stream<Item = antrelay::tracing::Event>> {
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_span_events(FmtSpan::FULL ^ FmtSpan::NEW);

    let stderr_layer = {
        cfg_if::cfg_if! {
            if #[cfg(debug_assertions)] {
                stderr_layer.pretty()
            } else {
                stderr_layer.json()
            }
        }
    };

    let (tx, rx) = smol::channel::unbounded();

    let level_filter = mk_level_filter();
    bootstrap!("enabling tracing with filter directive: {}", level_filter);

    tracing_subscriber::registry()
        .with(level_filter)
        .with(stderr_layer)
        .with(antrelay::tracing::EventStream::new(tx))
        .init();

    Ok(rx)
}

fn mk_level_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let default_str = {
            cfg_if::cfg_if! {
                if #[cfg(not(debug_assertions))] {
                    "warn,antrelay=info,relay=info"
                } else {
                    "info,antrelay=debug,relay=debug"
                }
            }
        };

        EnvFilter::from_str(default_str).expect("parsing envfilter default string")
    })
}
