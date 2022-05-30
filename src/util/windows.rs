pub fn signals() -> eyre::Result<smol::channel::Receiver<!>> {
    let (tx, rx) = smol::channel::bounded(1);

    ctrlc::set_handler(move || {
        tx.close();
    })?;

    Ok(rx)
}
