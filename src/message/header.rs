use packed_struct::prelude::*;
use tap::Conv;

use crate::{
    message::{
        MagicValue,
        UniqueId,
    },
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
    pub ty:          Type,
}

impl Header {
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
            self.ty.kind,
            self.ty.ack.then(|| "[Ack]").unwrap_or(""),
            self.ty.acked_message_invalid.then(|| "[Invalid]").unwrap_or(""),
        );

        out.push_str(&format!(
            "{:?} {:?} -> {:?} @ {} [{}]",
            ty_str,
            self.ty.source,
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
pub struct Type {
    #[packed_field(size_bits = "1")]
    pub ack: bool,

    #[packed_field(size_bits = "1")]
    pub acked_message_invalid: bool,

    #[packed_field(size_bits = "2", ty = "enum")]
    pub source: Source,

    #[packed_field(size_bits = "4", ty = "enum")]
    pub kind: Kind,
}

impl Type {
    pub const PONG: Self = Self {
        ack:                   true,
        acked_message_invalid: false,
        source:                Source::Frontend,
        kind:                  Kind::Ping,
    };
}

/// The target of this *type* of message. A discriminant.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PrimitiveEnum_u8)]
#[repr(u8)]
pub enum Source {
    Ant            = 0x0,
    CentralStation = 0x1,
    Frontend       = 0x2,
    Rover          = 0x3,
    Lander         = 0x4,
    Ground         = 0x5,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PrimitiveEnum_u8)]
#[repr(u8)]
pub enum Kind {
    Ping            = 0x01,
    Start           = 0x02,
    Calibrate       = 0x03,
    RoverWillTurn   = 0x04,
    RoverNotTurning = 0x05,
    VoltageSupplied = 0x06,
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
        fn type_strategy()(ack in any::<bool>(), acked_message_invalid in any::<bool>(), target in target_strategy(), kind in kind_strategy()) -> Type {
            Type {
                ack,
                acked_message_invalid,
                source,
                kind,
            }
        }
    }

    fn dest_strategy() -> impl Strategy<Value = Destination> {
        prop_oneof![
            Just(Destination::Ant),
            Just(Destination::CentralStation),
            Just(Destination::Frontend),
            Just(Destination::Ground),
        ]
    }

    fn target_strategy() -> impl Strategy<Value = Source> {
        prop_oneof![Just(Source::Ant), Just(Source::CentralStation), Just(Source::Frontend),]
    }

    fn kind_strategy() -> impl Strategy<Value = Kind> {
        prop_oneof![
            Just(Kind::Ping),
            Just(Kind::Start),
            Just(Kind::Calibrate),
            Just(Kind::RoverWillTurn),
            Just(Kind::RoverNotTurning),
            Just(Kind::VoltageSupplied),
        ]
    }
}
