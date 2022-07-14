use bytes::{
    Bytes,
    BytesMut,
};
use tokio_util::codec::{
    Decoder,
    Encoder,
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("cobs codec failed")]
    UnspecifiedCobsFailure,

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Default, Hash, PartialEq, Eq)]
pub struct CobsCodec;

impl<T> Encoder<T> for CobsCodec
where
    T: AsRef<[u8]>,
{
    type Error = Error;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let item = item.as_ref();

        let old_len = dst.len();
        dst.resize(old_len + cobs::max_encoding_length(item.len()), 0);

        let count = cobs::encode(item, &mut dst[old_len..]);
        dst.truncate(old_len + count);

        Ok(())
    }
}

impl Decoder for CobsCodec {
    type Error = Error;
    type Item = Bytes;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let report =
            cobs::decode_in_place_report(src).map_err(|_| Error::UnspecifiedCobsFailure)?;

        let mut result = src.split_to(report.src_used);
        result.truncate(report.dst_used);

        Ok(Some(result.freeze()))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use futures::StreamExt;
    use proptest::prelude::*;
    use tokio_util::codec::{
        FramedRead,
        FramedWrite,
    };

    async fn assert_encode_equals(
        input: impl IntoIterator<Item = impl AsRef<[u8]>>,
        expect: impl IntoIterator<Item = u8>,
    ) -> eyre::Result<()> {
        let mut writer = FramedWrite::new(vec![], CobsCodec);

        futures::stream::iter(input.into_iter().map(|x| Ok(x.as_ref().to_vec())))
            .forward(&mut writer)
            .await?;

        let expect = expect.into_iter().collect::<Vec<_>>();

        assert_eq!(expect, writer.into_inner());

        Ok(())
    }

    #[tokio::test]
    async fn encode_test() -> eyre::Result<()> {
        assert_encode_equals(&[&[0]], vec![1, 1]).await?;
        Ok(())
    }

    #[tokio::test]
    async fn decode_test() -> eyre::Result<()> {
        let input = vec![1, 1, 0, 0];
        let reader = FramedRead::new(&input[..], CobsCodec);

        // let mut result = reader.collect::<Vec<Result<Bytes, _>>>().await;
        let mut result: Vec<eyre::Result<Bytes>> = vec![];

        let first = result.pop().unwrap()?;
        assert_eq!(&[0, 3], first.as_ref());

        Ok(())
    }

    proptest! {
        #[test]
        fn test_enc_dec(input in any::<Vec<u8>>()) {

        }
    }
}
