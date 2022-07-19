use packed_struct::prelude::*;
use tap::Conv;

use crate::{
    MagicValue,
    MissionEpoch,
    Params,
    UniqueId,
};

lazy_static::lazy_static! {
    pub static ref SIZE_BYTES: usize = {
        use packed_struct::PackedStructInfo;

        let bit_size = Header::packed_bits();

        debug_assert_eq!(bit_size % 8, 0);
        bit_size / 8
    };
}

pub type Magic = MagicValue<0xeb>;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PackedStruct)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "8", endian = "lsb")]
pub struct Header {
    #[packed_field(size_bytes = "1")]
    pub magic:       Magic,
    #[packed_field(size_bytes = "1", ty = "enum")]
    pub destination: Destination,
    #[packed_field(size_bytes = "4")]
    pub timestamp:   MissionEpoch,
    pub seq:         u8,
    #[packed_field(size_bytes = "1")]
    pub ty:          MessageType,
}

impl Header {
    #[inline]
    pub fn log(env: &Params, kind: Event) -> Self {
        let mut result = Header::downlink(env, kind);
        result.ty = MessageType::PONG;

        result
    }

    #[inline]
    pub fn downlink(env: &Params, kind: Event) -> Self {
        Header {
            magic:       Default::default(),
            destination: Destination::Ground,

            timestamp: env.time,
            seq:       env.seq,

            ty: MessageType {
                disposition:         Disposition::Command,
                request_was_invalid: false,
                server:              Server::Frontend,
                event:               kind,
            },
        }
    }

    pub fn command(env: &Params, dest: Destination, server: Server, event: Event) -> Self {
        Self {
            magic:       Default::default(),
            destination: dest,

            timestamp: env.time,
            seq:       env.seq,

            ty: MessageType {
                disposition: Disposition::Command,
                request_was_invalid: false,
                server,
                event,
            },
        }
    }

    #[inline]
    pub fn unique_id(&self) -> UniqueId {
        UniqueId {
            timestamp: self.timestamp,
            seq:       self.seq,
        }
    }

    #[inline]
    pub fn display(&self) -> String {
        let mut out = String::new();

        let ty_str = format!(
            "{:?}{}{}",
            self.ty.event,
            (self.ty.disposition == Disposition::Ack).then(|| "[Ack]").unwrap_or(""),
            self.ty.request_was_invalid.then(|| "[Invalid]").unwrap_or(""),
        );

        out.push_str(&format!(
            "{:?} {:?} -> {:?} @ {} [{}]",
            ty_str,
            self.ty.server,
            self.destination,
            self.timestamp.conv::<chrono::DateTime<chrono::Utc>>(),
            self.seq,
        ));

        out
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PrimitiveEnum_u8)]
#[repr(u8)]
pub enum Destination {
    Frontend       = 0x90,
    CentralStation = 0x91,
    Ant            = 0x92,
    Ground         = 0xde,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PackedStruct)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "1")]
pub struct MessageType {
    #[packed_field(size_bits = "1", ty = "enum")]
    pub disposition: Disposition,

    #[packed_field(size_bits = "1")]
    pub request_was_invalid: bool,

    #[packed_field(size_bits = "2", ty = "enum")]
    pub server: Server,

    #[packed_field(size_bits = "4")]
    pub event: Event,
}

impl MessageType {
    pub const PONG: Self = Self {
        disposition:         Disposition::Ack,
        request_was_invalid: false,
        server:              Server::Frontend,
        event:               Event::FE_PING,
    };
}

/// The direction of this message.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PrimitiveEnum_u8)]
#[repr(u8)]
pub enum Disposition {
    Command = 0x0,
    Ack     = 0x1,
}

/// The target of this *type* of message. A discriminant.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PrimitiveEnum_u8)]
#[repr(u8)]
pub enum Server {
    Ant            = 0x0,
    CentralStation = 0x1,
    Frontend       = 0x2,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, derive_more::Into, PackedStruct)]
#[packed_struct(size_bits = "4")]
pub struct Event {
    value: u8,
}

impl const From<u8> for Event {
    fn from(val: u8) -> Self {
        Event {
            value: val,
        }
    }
}

impl Event {
    pub const A_CALI: Self = 0x03.into();
    pub const A_PING: Self = 0x01.into();
    pub const A_START: Self = 0x02.into();
    pub const CS_GARAGE_OPEN: Self = 0x01.into();
    pub const CS_PING: Self = 0x04.into();
    pub const CS_ROVER_MOVE: Self = 0x03.into();
    pub const CS_ROVER_STOP: Self = 0x02.into();
    pub const FE_5V_SUP: Self = 0x04.into();
    #[cfg(debug_assertions)]
    pub const FE_CS_PING: Self = 0x0f.into();
    pub const FE_GARAGE_OPEN: Self = 0x01.into();
    pub const FE_PING: Self = 0x05.into();
    pub const FE_ROVER_MOVE: Self = 0x03.into();
    pub const FE_ROVER_STOP: Self = 0x02.into();
}

#[cfg(test)]
mod test {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn pack_unpack_equivalence(header in header_strategy()) {
            let packed = header.pack();
            assert!(packed.is_ok());
            let packed = packed.unwrap();

            let unpacked = Header::unpack(&packed);
            assert!(unpacked.is_ok());
            let unpacked = unpacked.unwrap();

            assert_eq!(header, unpacked);
        }

        #[test]
        fn unpack_pack_equivalence(data in any::<[u8; 8]>()) {
            match Header::unpack(&data) {
                Ok(hdr) => {
                    let packed = hdr.pack();
                    assert_eq!(Ok(data), packed);
                },
                Err(_e) => {},
            }
        }
    }

    prop_compose! {
        fn header_strategy()(ty in type_strategy(), seq in any::<u8>(), timestamp in any::<u32>(), destination in dest_strategy()) -> Header {
            Header {
                magic: Magic::INSTANCE,
                destination,
                timestamp: MissionEpoch::from(timestamp),
                seq,
                ty,
            }
        }
    }

    prop_compose! {
        fn type_strategy()(disposition in direction_strategy(), request_was_invalid in any::<bool>(), server in server_strategy(), event in any::<u8>()) -> MessageType {
            MessageType {
                disposition,
                request_was_invalid,
                server,
                event: (event & 0x0f).into(),
            }
        }
    }

    fn direction_strategy() -> impl Strategy<Value = Disposition> {
        prop_oneof![Just(Disposition::Ack), Just(Disposition::Command),]
    }

    fn dest_strategy() -> impl Strategy<Value = Destination> {
        prop_oneof![
            Just(Destination::Ant),
            Just(Destination::CentralStation),
            Just(Destination::Frontend),
            Just(Destination::Ground),
        ]
    }

    fn server_strategy() -> impl Strategy<Value = Server> {
        prop_oneof![Just(Server::Ant), Just(Server::CentralStation), Just(Server::Frontend),]
    }
}
