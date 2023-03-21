use std::str::FromStr;

use tracing_subscriber::{
    fmt::format::FmtSpan,
    prelude::*,
    EnvFilter,
};

use util::bootstrap;

pub fn init(pretty: bool) {
    let level_filter = mk_level_filter();
    bootstrap!("enabling tracing with filter directive: {}", level_filter);

    let stderr_layer =
        tracing_subscriber::fmt::layer().with_writer(std::io::stderr).with_target(false);

    #[allow(clippy::needless_late_init)]
    let s;

    cfg_if::cfg_if! {
        if #[cfg(all(tokio_unstable, debug_assertions))] {
            s = tracing_subscriber::registry().with(console_subscriber::spawn())
        } else {
            s = tracing_subscriber::registry()
        }
    };

    if pretty {
        s.with(stderr_layer.pretty().with_filter(level_filter)).init();
    } else {
        s.with(
            stderr_layer
                .with_line_number(false)
                .with_timer(())
                .with_span_events(FmtSpan::NONE)
                .with_filter(level_filter),
        )
        .init();
    }
}

fn mk_level_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let default_str = {
            cfg_if::cfg_if! {
                if #[cfg(not(debug_assertions))] {
                    "warn,antrelay=info,relay=info,antrelay-net=info,antrelay-runtime=info,antrelay-codec=info,antrelay-message=info,antrelay-util=info"
                } else {
                    "info,antrelay=debug,relay=debug,antrelay-net=debug,antrelay-runtime=debug,antrelay-codec=debug,antrelay-message=debug,antrelay-util=debug"
                }
            }
        };

        EnvFilter::from_str(default_str).expect("parsing envfilter default string")
    })
}
