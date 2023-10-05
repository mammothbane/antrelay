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
fn mk_ant_command(event: Event) -> eyre::Result<String> {
    let msg = message::command(&NULL_PARAMS, Destination::Ant, event);
    let result = msg.pack_to_vec()?;

    Ok(hex::encode(result))
}

#[inline]
fn print_command(event: &str, msg: &str) -> eyre::Result<()> {
    eprintln!("{msg}");
    println!("{event}");
    eprintln!();

    Ok(())
}

fn main() -> eyre::Result<()> {
    eprintln!("hex-encoding of binary control messages (rover -> antrelay frontend)");

    [
        ("5v supplied to ant", Event::FEPowerSupplied),
        ("ant garage open", Event::FEGarageOpen),
        ("rover is not turning", Event::FERoverStop),
        ("rover is turning", Event::FERoverMove),
    ]
    .into_iter()
        .map(|(msg, event)| (msg, mk_fe_command(event)))
        .try_for_each(|(msg, event)| {
            let evt = event?;
            print_command(&evt, msg)
        })?;

    eprintln!("hex-encoding of selected binary ant control messages");

    [
        ("ant start", Event::AntStart),
        ("ant stop", Event::AntStop),
        ("ant forward (test only)", Event::AntMoveForward),
        ("ant backward (test only)", Event::AntMoveBackward),
        ("ant ping", Event::AntPing),
    ]
        .into_iter()
        .map(|(msg, event)| (msg, mk_ant_command(event)))
        .try_for_each(|(msg, event)| {
            let evt = event?;
            print_command(&evt, msg)
        })?;

    Ok(())
}
