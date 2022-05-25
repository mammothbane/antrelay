use packed_struct::prelude::*;

lazy_static::lazy_static! {
    pub static ref SIZE_BYTES: usize = {
        use packed_struct::PackedStructInfo;

        let bit_size = Header::packed_bits();

        debug_assert_eq!(bit_size % 8, 0);
        bit_size / 8
    };
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PackedStruct)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "8", endian = "lsb")]
pub struct Header {
    pub magic:       u8,
    #[packed_field(size_bytes = "1", ty = "enum")]
    pub destination: Destination,
    pub _timestamp:  u32,
    pub seq:         u8,
    #[packed_field(size_bytes = "1")]
    pub ty:          Type,
}

impl Header {
    pub const MAGIC: u8 = 0xeb;
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
    #[packed_field(bits = "0")]
    pub ack: bool,

    #[packed_field(bits = "1..4", ty = "enum")]
    pub target: Target,

    #[packed_field(bits = "4..8", ty = "enum")]
    pub kind: Kind,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PrimitiveEnum_u8)]
#[repr(u8)]
pub enum Target {
    Ant            = 0x0,
    CentralStation = 0x1,
    Frontend       = 0x2,
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

            let unpacked = Header::unpack(packed);
            assert!(unpacked.is_ok());
            let unpacked = unpacked.unwrap();

            assert_eq!(header, unpacked);
        }
    }

    prop_compose! {
        fn header_strategy()(ty in type_strategy(), seq in any::<u8>(), _timestamp in any::<u32>(), destination in dest_strategy()) -> impl Strategy<Value = Header> {
            Header {
                magic: Header::MAGIC,
                destination,
                _timestamp,
                seq,
                ty,
            }
        }
    }

    prop_compose! {
        fn type_strategy()(ack in any::<bool>(), target in target_strategy(), kind in kind_strategy()) -> impl Strategy<Value = Type> {
            Type {
                ack,
                target,
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

    fn target_strategy() -> impl Strategy<Value = Target> {
        prop_oneof![Just(Target::Ant), Just(Target::CentralStation), Just(Target::Frontend),]
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
