pub trait Checksum {
    type Output: num_traits::PrimInt;

    fn checksum(vals: &[u8]) -> Self::Output;
    fn checksum_array(vals: &[u8]) -> smallvec::SmallVec<[u8; 8]>;
}

#[inline]
pub const fn size<T>() -> usize
where
    T: Checksum,
{
    std::mem::size_of::<T::Output>()
}

#[macro_export]
macro_rules! impl_checksum {
    ($vis:vis $name:ident, u8, $algo:expr) => {
        $crate::impl_checksum!($vis $name, u8, $algo, vals, {
            let mut ret = ::smallvec::SmallVec::new();
            ret.push(Self::checksum(vals));

            ret
        });
    };

    ($vis:vis $name:ident, $ty:ty, $algo:expr) => {
        $crate::impl_checksum!($vis $name, $ty, $algo, ::byteorder::BE);
    };

    ($vis:vis $name:ident, $ty:ty, $algo:expr, $endian:ty) => {
        $crate::impl_checksum!($vis $name, $ty, $algo, vals, {
            paste::paste! {
                let mut ret = ::smallvec::smallvec![0u8; ::std::mem::size_of::<$ty>()];

                <$endian as ::byteorder::ByteOrder>::[< write_ $ty >](&mut ret[..], Self::checksum(vals));
                ret
            }
        });
    };

    ($vis:vis $name:ident, $ty:ty, $algo:expr, $vals:ident, $array_body:expr) => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize)]
        $vis struct $name;

        impl $crate::message::util::Checksum for $name {
            type Output = $ty;

            fn checksum(vals: &[u8]) -> Self::Output {
                const INSTANCE: ::crc::Crc<$ty> = ::crc::Crc::<$ty>::new(&$algo);

                INSTANCE.checksum(vals)
            }

            fn checksum_array($vals: &[u8]) -> smallvec::SmallVec<[u8; 8]> {
                $array_body
            }
        }
    };
}

pub use impl_checksum;

#[cfg(test)]
mod test {
    use byteorder::ByteOrder;
    use num_traits::FromPrimitive;
    use proptest::prelude::*;

    use super::*;

    impl_checksum!(pub U8CompileTest,  u8, crc::CRC_8_SMBUS);

    impl_checksum!(pub U16CompileTest, u16, crc::CRC_16_DNP);
    impl_checksum!(pub U24CompileTest, u32, crc::CRC_24_OPENPGP);
    impl_checksum!(pub U32CompileTest, u32, crc::CRC_32_AIXM);
    impl_checksum!(pub U64CompileTest, u64, crc::CRC_64_MS);

    impl_checksum!(pub U16CompileTestLittle, u16, crc::CRC_16_DNP, ::byteorder::LittleEndian);
    impl_checksum!(pub U24CompileTestLittle, u32, crc::CRC_24_OPENPGP, ::byteorder::LittleEndian);
    impl_checksum!(pub U32CompileTestLittle, u32, crc::CRC_32_AIXM, ::byteorder::LittleEndian);
    impl_checksum!(pub U64CompileTestLittle, u64, crc::CRC_64_MS, ::byteorder::LittleEndian);

    fn basic_function_helper<C: Checksum, B: ByteOrder>(to_checksum: &[u8])
    where
        C::Output: std::fmt::Debug + FromPrimitive,
    {
        let int = C::checksum(to_checksum);
        let array = C::checksum_array(to_checksum);

        let size = std::mem::size_of::<C::Output>();
        assert_eq!(array.len(), size);

        let reread = B::read_uint(&array, size);
        let reread = C::Output::from_u64(reread).unwrap();
        assert_eq!(int, reread);
    }

    proptest! {
        #[test]
        fn u8(to_checksum in any::<Vec<u8>>()) {
            let int = U8CompileTest::checksum(&to_checksum);
            let array = U8CompileTest::checksum_array(&to_checksum);
            assert_eq!(array.len(), 1);
            assert_eq!(int, array[0]);
        }

        #[test]
        fn u16(to_checksum in any::<Vec<u8>>()) {
            basic_function_helper::<U16CompileTest, ::byteorder::BE>(&to_checksum);
            basic_function_helper::<U16CompileTestLittle, ::byteorder::LE>(&to_checksum);
        }

        #[test]
        fn u24(to_checksum in any::<Vec<u8>>()) {
            basic_function_helper::<U24CompileTest, ::byteorder::BE>(&to_checksum);
            basic_function_helper::<U24CompileTestLittle, ::byteorder::LE>(&to_checksum);
        }

        #[test]
        fn u32(to_checksum in any::<Vec<u8>>()) {
            basic_function_helper::<U32CompileTest, ::byteorder::BE>(&to_checksum);
            basic_function_helper::<U32CompileTestLittle, ::byteorder::LE>(&to_checksum);
        }

        #[test]
        fn u64(to_checksum in any::<Vec<u8>>()) {
            basic_function_helper::<U64CompileTest, ::byteorder::BE>(&to_checksum);
            basic_function_helper::<U64CompileTestLittle, ::byteorder::LE>(&to_checksum);
        }
    }
}
