use std::ops::DerefMut;
use async_std::sync;
use log::{
    Metadata,
    Record,
    LevelFilter,
};
use smol::io::AsyncWriteExt;
use smol::net::unix::UnixDatagram;
use fern::colors::{
    Color, ColoredLevelConfig,
};

lazy_static::lazy_static! {
    static ref COLOR_CONFIG: ColoredLevelConfig = ColoredLevelConfig::new()
        .info(Color::Green)
        .debug(Color::BrightBlue)
        .trace(Color::BrightMagenta);

    static ref STDIO_DISPATCH: anyhow::Result<fern::Dispatch> = fern::Dispatch::new()
        .level(LevelFilter::Info)
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{} [{}] [{}] {}",
                chrono::Local::now().format("%_m/%_d/%y %l:%M:%S%P"),
                colors.color(record.level()),
                record.target(),
                message
            ))
        })
        .chain(std::io::stderr())
        .apply()
        .map_err(anyhow::Error::from);
}

pub fn init(
    telemetry: UnixDatagram,
    reliable: UnixDatagram,
    store_and_forward: UnixDatagram,
) -> anyhow::Result<()> {
    let logger = UDSLogger {
        out: sync::Mutex::new(store_and_forward),
    };

    fern::Dispatch::new()
        .chain(logger)
        .chain(STDIO_DISPATCH?)
        .apply()?;

    Ok(())
}

struct UDSLogger {
    out: sync::Mutex<UnixDatagram>,
}

fn serialize_log(record: &Record) -> anyhow::Result<Vec<u8>> {
    todo!()
}

impl log::Log for UDSLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let result: anyhow::Result<()> = smol::block_on(async move {
            let mut out = self.out.lock().await;
            out.send(&serialize_log(record)?).await?;

            Ok(())
        });

        if let Err(e) = result {
            eprintln!("failed to send log: {}", e);
        }
    }

    fn flush(&self) {
        smol::block_on(async {
            let mut out = self.out.lock().await;
            out.flush();
        })
    }
}
