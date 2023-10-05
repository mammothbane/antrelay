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

## rover -> frontend control messages
To produce a listing of hex-encoded control messages the frontend software expects to receive from
the rover at the relevant points in the mission, you can run:

```console
$ cargo run --bin rovermsgs
hex-encoding of binary control messages (rover -> antrelay frontend)
5v supplied to ant
eb900000000000240000000000000000009a

ant garage open
eb9000000000002100000000000000000055

rover is not turning
eb90000000000022000000000000000000ed

rover is turning
eb9000000000002300000000000000000085
```

Output as of 2/2023 is included above and is expected to be stable.

Additional output including direct ant control messages (*not to be sent by rover on mission*):

```console
# ...
hex-encoding of selected binary ant control messages
ant start
eb92000000000002000000000000000000ca

ant stop
eb92000000000005000000000000000000d5

ant forward (test only)
eb920000000000090000000000000000003b

ant backward (test only)
eb9200000000000a00000000000000000083

ant ping
eb9200000000000100000000000000000072
```

For clarity, we expect to receive these messages as binary packets over the uplink socket (not
textual hex). You can convert the messages to binary files as follows:

```shell
$ echo -n "$hex" | xxd -r -p > test.bin
```

## downlink
Over the downlink, the frontend echoes every packet it:

- receives over the uplink
- sends to the serial port
- receives from the serial port

Raw literal packets are always sent. If the frontend succeeds in (de)serializing the packet, it also
reencodes it and sends that as a "non-raw" version. E.g., there's both an "uplink raw" message type
(UDP packets from uplink) and an "uplink" (reencoded messages) type, to give us visibility in the
case of a serialization failure.

Our telemetry from the central station and ant is in the form of "serial downlink" packets that come
back over this link.

The downlink itself is encoded using the rust [`bincode`](https://docs.rs/bincode/latest/bincode/)
library and then compressed using brotli -- it is not easily introspectable.

### LO integration testing
If testing the messages defined above over the uplink, we provide the `decode_downlink` tool to
decode messages downlinked from the ant's docker container:

```shell
$ cargo run --bin docker_downlink < packet.bin
```

It expects binary input by default, but can be passed `--hex` or `--base64` as appropriate, e.g.:

```shell
$ echo -n $pkt_hex | cargo run --bin docker_downlink -- --hex
```

The most interesting messages will be marked `SERIAL DOWNLINK`, as this is what is coming back from
the central station:

- After sending 5v supplied, we expect periodic `CSPing[Ack]` messages with no payload.

- After sending ant garage open:
  - `CSGarageOpen[Ack]` with no payload. If the ant is not present, this should be it.
  - With the ant:
    - `CSBLEConnect[Ack]` with no payload. (It may send a `CSBLEDisconnect[Ack]` first if it was
       already connected to the ant.)
    - `CSRelay` packets containing ant data

- After sending rover not turning, assuming ant is present:
  - `AntStart[Ack]`
  - Ant should start to move
  - After it completes its move, a dump of `CSRelay` packets containing ant data

- After sending rover turning, assuming ant is present:
  - `AntStop[Ack]`
  - Ant should stop moving (I don't believe the current firmware does this)

Note that resending a packet from the rover will not cause repeat behavior unless we've been in a
different state since the first message. E.g. "not turning -> not turning" means the frontend
software will only trigger sending a message to the ant once.

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
- integration tests
