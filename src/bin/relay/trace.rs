use std::str::FromStr;

use tracing_subscriber::{
    fmt::format::FmtSpan,
    prelude::*,
    EnvFilter,
};

use util::bootstrap;

pub fn init(pretty: bool) {
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_span_events(FmtSpan::CLOSE)
        .with_target(false);

    let level_filter = mk_level_filter();
    bootstrap!("enabling tracing with filter directive: {}", level_filter);

    let s = tracing_subscriber::registry().with(level_filter);

    if pretty {
        s.with(stderr_layer.pretty()).init();
    } else {
        s.with(
            stderr_layer
                .with_line_number(false)
                .with_target(false)
                .with_timer(())
                .with_span_events(FmtSpan::NONE),
        )
        .init();
    }
}

fn mk_level_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let default_str = {
            cfg_if::cfg_if! {
                if #[cfg(not(debug_assertions))] {
                    "warn,antrelay=info,relay=info,antrelay-net=info,antrelay-tracing=info,antrelay-runtime=info,antrelay-codec=info,antrelay-message=info,antrelay-util=info"
                } else {
                    "info,antrelay=debug,relay=debug,antrelay-net=debug,antrelay-tracing=debug,antrelay-runtime=debug,antrelay-codec=debug,antrelay-message=debug,antrelay-util=debug"
                }
            }
        };

        EnvFilter::from_str(default_str).expect("parsing envfilter default string")
    })
}
