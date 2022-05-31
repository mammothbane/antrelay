use signal_hook::{
    consts::*,
    iterator::Signals,
};
use std::thread;

pub fn signals() -> eyre::Result<smol::channel::Receiver<!>> {
    let (tx, rx) = smol::channel::bounded(1);
    let mut signals = Signals::new(&[SIGTERM, SIGINT])?;

    thread::spawn(move || {
        signals.wait().next();

        tx.close();
    });

    Ok(rx)
}
