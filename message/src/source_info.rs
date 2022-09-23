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

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SourceInfo {
    Empty,
    Info {
        header:   Header,
        checksum: u8,
    },
}

impl SourceInfo {
    const EMPTY: [u8; Self::SIZE] = [0u8; Self::SIZE];
    const SIZE: usize = 8 + 1;

    pub fn invalid(&self) -> bool {
        match self {
            Self::Empty => false,
            Self::Info {
                header,
                ..
            } => header.ty.invalid,
        }
    }
}

impl PackedStruct for SourceInfo {
    type ByteArray = [u8; Self::SIZE];

    fn pack(&self) -> PackingResult<Self::ByteArray> {
        let mut b = [0u8; Self::SIZE];

        match self {
            Self::Info {
                header,
                checksum,
            } => {
                header.pack_to_slice(&mut b[..Self::SIZE - 1])?;
                b[Self::SIZE - 1] = *checksum;
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

                Ok(Self::Info {
                    header,
                    checksum: *crc,
                })
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
            Self::Info {
                header,
                checksum,
            } => write!(f, "Source <{} crc 0x{:#x}>", header.display(), checksum),
        }
    }
}
