# high-level architecture/organization notes

## runtime
The major organizational item you'll notice is that I used an actor system (`actix`) for this
project. I did this because:

- There are multiple independent control loops (serial, rover, control state machine) that require
  independent supervision -- each should restart if it fails without tearing down the whole process
  (as this downtime could lead to (partial) message loss)
- In-process message broker/bus functionality makes modularizing/decoupling straightforward --
  anything can send a message to the broker or monitor any message type passing through the broker.
  This made it very easy, for instance, to relay all messages received by the frontend (through any
  channel) back over the downlink (see `runtime/src/ground/downlink.rs`)
- Testing is easy because everything is naturally decoupled and communicates exclusively via
  messages

## crates

### `codec`
Implements several codecs. These convert `tokio::io::Async{Read,Write}` types (in this case, read:
serial ports) into `futures::{Stream, Sink}` by implementing framing. Currently just the `CobsCodec`
is used (for framing packets over the serial connection).

### `message`
Declarative messages, using an external crate (`packed_struct`) to map our data format onto Rust
`struct`s. A standard message (in any direction except downlink to rover -- uplink, serial up,
serial down) is organized as:

```
WithCRC
    L CRC payload
        L Header
            L Magic byte
            L Timestamp
            L Sequence number
            L Message type
        L Message payload (Bytes)
    L CRC (u8)
```

#### Downlink messages (`message/src/downlink/mod.rs`)
Downlink messages are serialized using Rust's `serde` with `bincode` as the serializer. (They are
also compressed, but the `runtime` handles this.)

#### Time: `MissionEpoch`
Our message format specifies that timestamps will be 4 bytes and expressed in milliseconds. This
gives us just shy of 50 days of resolution before we wrap. As such, we chose to pick a canonical
reference instant for the mission, ideally set just prior to a firm launch date. E.g., if we know
that the launch is on 1/1/2023, we would set the epoch to midnight UTC on 12/31/2022.

(Even if this adjustment is not possible due to LO requirements on software freeze, as long as we
record all of our downlink packets on the ground with their reception time, disambiguation around a
rollover will be straightforward.)

The `MissionEpoch` type knows about the reference epoch instant and handles conversions to/from
Rust-native `std::time` and `chrono` types.

### `net`
Implements abstractions over datagram socket types to permit OS-independent development. It's
more convenient for me to develop on Windows, but UDS aren't available, so I use IP sockets instead
(and test on Linux periodically).

### `runtime`
The core functionality of the system. Implements all the actor types as well as some utilities
around `actix`.

- `runtime/src/ground` deals with the socket links to the rover. Note that downlink packets are
  brotli-compressed.
- `runtime/src/serial` handles the serial port connection, with `RawIO` in charge of the IO with the
  port (accepting and producing unformatted packets), `Serial` performing packing/unpacking messages
  going to/from `RawIO`, and `Commander` implementing the notion of a "request" to the serial port
  that expects an ack message.
- `runtime/src/system` include services (actor system singletons) and a utility to replace them at
  runtime for mocking purposes.
- `runtime/src/state_machine.rs` implements the state machine that the frontend runs to control the
  ant.

### `util`
Various location-agnostic utilities.

### `src` (`antrelay` crate)
Where the binaries live. They handle all the user-interfacing required to setup and drive `runtime`.
