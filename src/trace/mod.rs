pub mod downlink;

use std::str::FromStr;

use tracing_subscriber::{
    fmt::format::FmtSpan,
    prelude::*,
    EnvFilter,
};

use util::bootstrap;

pub fn init(pretty: bool) {
    let console_filter = console_filter();
    bootstrap!("enabling tracing with filter directive: {}", console_filter);

    let stderr_layer =
        tracing_subscriber::fmt::layer().with_writer(std::io::stderr).with_target(false);

    let downlink_layer = downlink::Layer.with_filter(downlink_filter());

    #[allow(clippy::needless_late_init)]
    let s;

    cfg_if::cfg_if! {
        if #[cfg(all(tokio_unstable, debug_assertions))] {
            s = tracing_subscriber::registry().with(console_subscriber::spawn())
        } else {
            s = tracing_subscriber::registry()
        }
    }

    let s = s.with(downlink_layer);

    if pretty {
        s.with(stderr_layer.pretty().with_filter(console_filter)).init();
    } else {
        s.with(
            stderr_layer
                .with_line_number(false)
                .with_timer(())
                .with_span_events(FmtSpan::NONE)
                .with_filter(console_filter),
        )
        .init();
    }
}

fn console_filter() -> EnvFilter {
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

fn downlink_filter() -> EnvFilter {
    EnvFilter::from_str("info").expect("parsing envfilter")
}
