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

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, Hash, PackedStruct, serde::Serialize, serde::Deserialize,
)]
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

        result.ty = MessageType {
            disposition: Disposition::Ack,
            invalid:     false,
            event:       Event::FEPing,
        };

        result
    }

    #[inline]
    pub fn downlink(env: &Params, event: Event) -> Self {
        Header {
            magic:       Default::default(),
            destination: Destination::Ground,

            timestamp: env.time,
            seq:       env.seq,

            ty: MessageType {
                disposition: Disposition::Command,
                invalid: false,
                event,
            },
        }
    }

    pub fn command(env: &Params, dest: Destination, event: Event) -> Self {
        Self {
            magic:       Default::default(),
            destination: dest,

            timestamp: env.time,
            seq:       env.seq,

            ty: MessageType {
                disposition: Disposition::Command,
                invalid: false,
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
            self.ty.invalid.then(|| "[Invalid]").unwrap_or(""),
        );

        let ts = self.timestamp.conv::<chrono::DateTime<chrono::Utc>>();
        let ts_fmt = ts.format("%y/%m/%d %TZ");

        out.push_str(&format!(
            "{} [seq {:?}]: {} -> {:?}",
            ts_fmt, self.seq, ty_str, self.destination
        ));

        out
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, Hash, PrimitiveEnum_u8, serde::Serialize, serde::Deserialize,
)]
#[repr(u8)]
pub enum Destination {
    Frontend       = 0x90,
    CentralStation = 0x91,
    Ant            = 0x92,
    Ground         = 0xde,
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, Hash, PackedStruct, serde::Serialize, serde::Deserialize,
)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "1")]
pub struct MessageType {
    #[packed_field(size_bits = "1", ty = "enum")]
    pub disposition: Disposition,

    #[packed_field(size_bits = "1")]
    pub invalid: bool,

    #[packed_field(size_bits = "6", ty = "enum")]
    pub event: Event,
}

/// The direction of this message.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, Hash, PrimitiveEnum_u8, serde::Serialize, serde::Deserialize,
)]
#[repr(u8)]
pub enum Disposition {
    Command = 0x0,
    Ack     = 0x1,
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, Hash, PrimitiveEnum_u8, serde::Serialize, serde::Deserialize,
)]
#[repr(u8)]
pub enum Event {
    AntPing         = 0x01,
    AntStart        = 0x02,
    AntCalibrate    = 0x03,
    AntOTA          = 0x04,
    AntStop         = 0x05,

    // test commands:
    AntPowerOff     = 0x06,
    AntHeaterOn     = 0x07,
    AntHeaterOff    = 0x08,
    AntMoveForward  = 0x09,
    AntMoveBackward = 0x0a,

    CSGarageOpen    = 0x11,
    CSRoverStop     = 0x12,
    CSRoverMove     = 0x13,
    CSPing          = 0x14,
    CSDFUViaBLE     = 0x15,
    CSDFUSerial     = 0x16,
    CSAntDFUInit    = 0x17,
    CSAntDFUPacket  = 0x18,

    CSRelay         = 0x19,
    CSBLEConnect    = 0x1a,
    CSBLEDisconnect = 0x1b,

    FEGarageOpen    = 0x21,
    FERoverStop     = 0x22,
    FERoverMove     = 0x23,
    FEPowerSupplied = 0x24,
    FEPing          = 0x25,

    #[cfg(debug_assertions)]
    DebugCSPing     = 0x2f,
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
        fn type_strategy()(disposition in direction_strategy(), invalid in any::<bool>(), event in event_strategy()) -> MessageType {
            MessageType {
                disposition,
                invalid,
                event,
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

    fn event_strategy() -> impl Strategy<Value = Event> {
        prop_oneof![
            Just(Event::AntPing),
            Just(Event::AntStart),
            Just(Event::AntCalibrate),
            Just(Event::AntOTA),
            Just(Event::AntStop),
            Just(Event::AntPowerOff),
            Just(Event::AntPowerOff),
            Just(Event::AntHeaterOn),
            Just(Event::AntHeaterOff),
            Just(Event::AntMoveForward),
            Just(Event::AntMoveBackward),
            Just(Event::CSGarageOpen),
            Just(Event::CSRoverStop),
            Just(Event::CSRoverMove),
            Just(Event::CSPing),
            Just(Event::CSDFUViaBLE),
            Just(Event::CSDFUSerial),
            Just(Event::CSAntDFUInit),
            Just(Event::CSAntDFUPacket),
            Just(Event::CSRelay),
            Just(Event::CSBLEConnect),
            Just(Event::CSBLEDisconnect),
            Just(Event::FEGarageOpen),
            Just(Event::FERoverStop),
            Just(Event::FERoverMove),
            Just(Event::FEPowerSupplied),
            Just(Event::FEPing),
            #[cfg(debug_assertions)]
            Just(Event::DebugCSPing),
        ]
    }
}
