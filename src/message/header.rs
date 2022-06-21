use packed_struct::prelude::*;
use tap::Conv;

use crate::{
    message::{
        MagicValue,
        UniqueId,
    },
    util::PacketEnv,
    MissionEpoch,
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
    pub ty:          RequestMeta,
}

impl Header {
    #[inline]
    pub fn downlink(env: &PacketEnv, kind: Conversation) -> Self {
        Header {
            magic:       Default::default(),
            destination: Destination::Ground,

            timestamp: env.clock.now(),
            seq:       env.seq.next(),

            ty: RequestMeta {
                disposition:         Disposition::Command,
                request_was_invalid: false,
                server:              Server::Frontend,
                conversation_type:   kind,
            },
        }
    }

    pub fn fe_command(env: &PacketEnv, kind: Conversation) -> Self {
        Header {
            magic:       Default::default(),
            destination: Destination::CentralStation,

            timestamp: env.clock.now(),
            seq:       env.seq.next(),

            ty: RequestMeta {
                disposition:         Disposition::Command,
                request_was_invalid: false,
                server:              Server::Frontend,
                conversation_type:   kind,
            },
        }
    }

    pub fn cs_command(env: &PacketEnv, kind: Conversation) -> Self {
        Header {
            magic:       Default::default(),
            destination: Destination::CentralStation,

            timestamp: env.clock.now(),
            seq:       env.seq.next(),

            ty: RequestMeta {
                disposition:         Disposition::Command,
                request_was_invalid: false,
                server:              Server::CentralStation,
                conversation_type:   kind,
            },
        }
    }

    pub fn ant_command(env: &PacketEnv, kind: Conversation) -> Self {
        Header {
            magic:       Default::default(),
            destination: Destination::Ant,

            timestamp: env.clock.now(),
            seq:       env.seq.next(),

            ty: RequestMeta {
                disposition:         Disposition::Command,
                request_was_invalid: false,
                server:              Server::Ant,
                conversation_type:   kind,
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
            self.ty.conversation_type,
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
pub struct RequestMeta {
    #[packed_field(size_bits = "1", ty = "enum")]
    pub disposition: Disposition,

    #[packed_field(size_bits = "1")]
    pub request_was_invalid: bool,

    #[packed_field(size_bits = "2", ty = "enum")]
    pub server: Server,

    #[packed_field(size_bits = "4", ty = "enum")]
    pub conversation_type: Conversation,
}

impl RequestMeta {
    pub const PONG: Self = Self {
        disposition:         Disposition::Ack,
        request_was_invalid: false,
        server:              Server::Frontend,
        conversation_type:   Conversation::Ping,
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
    Rover          = 0x3,
    Lander         = 0x4,
    Ground         = 0x5,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PrimitiveEnum_u8)]
#[repr(u8)]
pub enum Conversation {
    Ping              = 0x01,
    Start             = 0x02,
    Calibrate         = 0x03,
    RoverMoving       = 0x04,
    RoverStopping     = 0x05,
    PowerSupplied     = 0x06,
    Relay             = 0x07,

    GarageOpenPending = 0x0b,
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
        fn type_strategy()(disposition in direction_strategy(), request_was_invalid in any::<bool>(), server in server_strategy(), conversation_type in conversation_strategy()) -> RequestMeta {
            RequestMeta {
                disposition,
                request_was_invalid,
                server,
                conversation_type,
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

    fn conversation_strategy() -> impl Strategy<Value = Conversation> {
        prop_oneof![
            Just(Conversation::Ping),
            Just(Conversation::Start),
            Just(Conversation::Calibrate),
            Just(Conversation::RoverMoving),
            Just(Conversation::RoverStopping),
            Just(Conversation::PowerSupplied),
        ]
    }
}
