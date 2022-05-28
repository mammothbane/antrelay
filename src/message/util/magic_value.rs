use std::{
    cmp::Ordering,
    hash::Hasher,
};

use packed_struct::{
    prelude::*,
    PackedStructInfo,
    PackingResult,
};

#[derive(Copy, Clone, Debug, Eq, Ord, Default)]
pub struct MagicValue<const C: u8>;

impl<const C: u8> MagicValue<C> {
    pub const INSTANCE: Self = Self;
    pub const VALUE: u8 = C;
}

impl<const C: u8> PackedStruct for MagicValue<C> {
    type ByteArray = [u8; 1];

    #[inline]
    fn pack(&self) -> PackingResult<Self::ByteArray> {
        Ok([C])
    }

    fn unpack(src: &Self::ByteArray) -> PackingResult<Self> {
        if src[..1] == [C][..] {
            Ok(Self)
        } else {
            Err(PackingError::InvalidValue)
        }
    }
}

impl<const C: u8> const PackedStructInfo for MagicValue<C> {
    #[inline]
    fn packed_bits() -> usize {
        std::mem::size_of::<u8>() * 8
    }
}

impl<const C: u8, const D: u8> PartialEq<MagicValue<D>> for MagicValue<C> {
    #[inline]
    fn eq(&self, _other: &MagicValue<D>) -> bool {
        C == D
    }
}

impl<const C: u8, const D: u8> PartialOrd<MagicValue<D>> for MagicValue<C> {
    fn partial_cmp(&self, _other: &MagicValue<D>) -> Option<Ordering> {
        C.partial_cmp(&D)
    }
}

impl<const C: u8> std::hash::Hash for MagicValue<C> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        C.hash(state)
    }
}

#[cfg(test)]
mod test {
    use proptest::prelude::*;

    use super::*;

    fn only_valid<const C: u8>(x: u8) {
        let packed = MagicValue::<C>::INSTANCE.pack().unwrap();
        assert_eq!(packed, [C]);

        let unpack_result = MagicValue::<C>::unpack(&[x]);

        match x {
            x if x == C => assert_eq!(unpack_result, Ok(MagicValue)),
            _otherwise => assert_eq!(unpack_result, Err(PackingError::InvalidValue)),
        }
    }

    proptest! {
        #[test]
        fn only_valid_0(x in any::<u8>()) {
            only_valid::<0>(x)
        }

        #[test]
        fn only_valid_1(x in any::<u8>()) {
            only_valid::<1>(x)
        }

        #[test]
        fn only_valid_254(x in any::<u8>()) {
            only_valid::<254>(x)
        }

        #[test]
        fn only_valid_255(x in any::<u8>()) {
            only_valid::<255>(x)
        }
    }
}
