use std::str::FromStr;

use smol::stream::Stream;
use tracing_subscriber::{
    fmt::format::FmtSpan,
    prelude::*,
    EnvFilter,
};

pub fn init() -> eyre::Result<impl Stream<Item = lunarrelay::util::Event>> {
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_span_events(FmtSpan::CLOSE);

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

    tracing_subscriber::registry()
        .with(mk_level_filter())
        .with(stderr_layer)
        .with(lunarrelay::util::EventStream::new(tx))
        .init();

    Ok(rx)
}

fn mk_level_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let default_str = {
            cfg_if::cfg_if! {
                if #[cfg(not(debug_assertions))] {
                    "warn,lunarrelay=info,relay=info"
                } else {
                    "info,lunarrelay=debug,relay=debug"
                }
            }
        };

        EnvFilter::from_str(default_str).expect("parsing envfilter default string")
    })
}
