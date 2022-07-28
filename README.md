# antrelay

## binaries

### antrelay (`src/main.rs`)

This is the frontend software. It connects to uplink and downlink Unix sockets,
as well as a serial port (the central station).

### console

An uplink/downlink simulator console. Connects to running `relay` software and
sends packets as if it were the rover.

### serial_console

Similar to `console`, emulates the serial side. Connects to a serial port and sends cobs-encoded
packets as if it were
the central station.

## windows

Development was a bit practically easier for me on Windows, so everything also runs on Windows -- in
this case, rather than Unix sockets, everything communicates over IP/UDP sockets.

## build / run

This project was written in Rust. Grab it from <https://rustup.rs>. To run a binary:

```console
$ cargo run --bin <binary_name> -- <args>
```

For testing, I recommend invoking `antrelay` and the `console` in different terminals:

```console
# relay
$ cargo run -- --serial-port <your serial port> --uplink socks/uplink --downlink socks/downlink

# console
$ cargo run --bin console -- --uplink socks/uplink --downlink socks/downlink
```

The console supports both submitting commands to the uplink, and decoding messages that come back
over the downlink.

Type `help` to list supported commands.

## debugging serial (`serial_console`)

If you want to test serial functionality independent of the hardware, use `serial_console`:

```console
$ cargo run --bin serial_console -- --serial_port <your serial port>
```

You'll probably need a null modem/loopback serial connection. The easiest way I've found to emulate
this is with
`socat`. It will report the ptys it allocates for you, to use with `serial_console` and `antrelay`:

```console
$ socat -d -d pty,raw,echo=0 pty,raw,echo=0
```

## docker (@LO)

A `docker run` oneliner:

```console
$ docker run \
    -v socket_dir:/sockets:rw \
    -v /dev/SERIAL_TTY:/serial:rw \
    --restart on-failure \
    antrelay:latest \
    --serial-port /serial \
    --baud 115200 \
    --uplink /sockets/uplink \
    --downlink /sockets/downlink
```

(Plus whatever is required to give us `dialout`/access to the serial port on the host system)

# todo
- restart socket and serial connections on io error
- logging for the frontend software
- integration tests
