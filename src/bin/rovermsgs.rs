#![feature(iterator_try_collect)]

use packed_struct::PackedStructSlice;

use message::{
    header::{
        Destination,
        Event,
    },
    MissionEpoch,
};

const NULL_PARAMS: message::Params = message::Params {
    seq:  0,
    time: MissionEpoch::new(0),
};

#[inline]
fn mk_fe_command(event: Event) -> eyre::Result<String> {
    let msg = message::command(&NULL_PARAMS, Destination::Frontend, event);
    let result = msg.pack_to_vec()?;

    Ok(hex::encode(result))
}

#[inline]
fn print_fe_command(event: Event, msg: &str) -> eyre::Result<()> {
    eprintln!("{}", msg);
    println!("{}", mk_fe_command(event)?);
    eprintln!();

    Ok(())
}

fn main() -> eyre::Result<()> {
    eprintln!("hex-encoding of binary control messages (rover -> antrelay frontend)");

    vec![
        ("5v supplied to ant", Event::FEPowerSupplied),
        ("ant garage open", Event::FEGarageOpen),
        ("rover is not turning", Event::FERoverStop),
        ("rover is turning", Event::FERoverMove),
    ]
    .into_iter()
    .map(|(msg, evt)| print_fe_command(evt, msg))
    .try_collect::<()>()?;

    Ok(())
}
