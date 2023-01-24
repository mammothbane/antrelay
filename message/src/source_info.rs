use std::fmt::{
    Display,
    Formatter,
};

use packed_struct::{
    PackedStruct,
    PackedStructInfo,
    PackedStructSlice,
    PackingResult,
};

use crate::Header;

#[repr(packed)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Info {
    pub header:   Header,
    pub checksum: u8,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SourceInfo {
    Empty,
    Info(Info),
}

impl SourceInfo {
    const EMPTY: [u8; Self::SIZE] = [0u8; Self::SIZE];
    const SIZE: usize = std::mem::size_of::<Info>();

    pub fn invalid(&self) -> bool {
        match self {
            Self::Empty => false,
            Self::Info(info) => info.header.ty.invalid,
        }
    }
}

impl PackedStruct for SourceInfo {
    type ByteArray = [u8; Self::SIZE];

    fn pack(&self) -> PackingResult<Self::ByteArray> {
        let mut b = [0u8; Self::SIZE];

        match self {
            Self::Info(info) => {
                let header = info.header;

                header.pack_to_slice(&mut b[..Self::SIZE - 1])?;
                b[Self::SIZE - 1] = info.checksum;
            },

            Self::Empty => b = Self::EMPTY,
        }

        Ok(b)
    }

    #[inline]
    fn unpack(src: &Self::ByteArray) -> PackingResult<Self> {
        match src {
            &Self::EMPTY => Ok(Self::Empty),
            [header @ .., crc] => {
                let header = <Header as PackedStructSlice>::unpack_from_slice(header)?;

                Ok(Self::Info(Info {
                    header,
                    checksum: *crc,
                }))
            },
        }
    }
}

impl PackedStructInfo for SourceInfo {
    #[inline]
    fn packed_bits() -> usize {
        Self::SIZE * 8
    }
}

impl Display for SourceInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "no source"),
            Self::Info(info) => {
                let header = info.header;
                write!(f, "Source <{} crc 0x{:#x}>", header.display(), info.checksum)
            },
        }
    }
}
