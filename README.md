# antrelay

## binaries

### relay
This is the frontend software. It connects to uplink and downlink Unix sockets,
as well as a serial port (the central station).

### console
An uplink/downlink simulator console. Connects to running `relay` software and
sends packets as if it were the rover.

### mockrover
Meant to exercise various systems on `relay`. Not entirely up-to-date at the moment.

## windows
Development was a bit practically easier for me on Windows, so everything also
runs on Windows -- in this case, rather than Unix sockets, everything communicates
over IP/UDP sockets.

## build / run
This was written in Rust. Grab it from https://rustup.rs. To run a binary:

```console
$ cargo run --bin <binary_name> -- <args>
```

For testing, I recommend invoking the `relay` and the `console` in different terminals:

```console
# relay
$ cargo run --bin relay -- --serial-port <your serial port> --baud <baud> --uplink socks/uplink --downlink socks/downlink

# console
$ cargo run --bin console -- --uplink socks/uplink --downlink socks/downlink
```

The console supports both submitting commands to the uplink, and decoding messages that come back over the downlink. Type `help` to list supported commands.

## not yet finished
- The frontend `PONG` format -- sends back all log statements in an unfilterred and uncompacted form. It works, but is going to change.

