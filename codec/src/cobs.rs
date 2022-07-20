use bytes::{
    Buf,
    Bytes,
    BytesMut,
};
use tokio_util::codec::{
    Decoder,
    Encoder,
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("cobs protocol error")]
    CobsProtocol,

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

    #[tracing::instrument(skip_all, fields(value = %hex::encode(item.as_ref())), level = "trace", err(Display))]
    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let item = item.as_ref();

        let old_len = dst.len();
        dst.resize(old_len + cobs::max_encoding_length(item.len()), 0);

        let count = cobs::encode(item, &mut dst[old_len..]);
        dst.resize(old_len + count + 1, 0);

        tracing::info!(encoded = %hex::encode(&dst[old_len..]), "cobs-encoded value");

        Ok(())
    }
}

impl Decoder for CobsCodec {
    type Error = Error;
    type Item = Bytes;

    #[tracing::instrument(skip_all, fields(src = %hex::encode(&*src)), level = "trace", err(Display))]
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() || !src.contains(&0) {
            return Ok(None);
        }

        let report = match cobs::decode_in_place_report(src) {
            Ok(report) => report,
            Err(_) => {
                if let Some(pos) = src.iter().position(|&x| x == 0) {
                    src.advance(pos + 1);
                }

                return Err(Error::CobsProtocol);
            },
        };

        let next_zero = if src[report.src_used..].starts_with(&[0]) {
            1
        } else {
            0
        };

        let mut result = src.split_to(report.src_used + next_zero);
        result.truncate(report.dst_used);

        tracing::info!(decoded = %hex::encode(&*result), "cobs-decoded value");

        Ok(Some(result.freeze()))
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if !buf.is_empty() && !buf.ends_with(&[0]) {
            buf.resize(buf.len() + 1, 0);
        }

        self.decode(buf)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::time::Duration;

    use futures::{
        AsyncWriteExt,
        StreamExt,
    };
    use proptest::prelude::*;
    use tokio_util::codec::{
        FramedRead,
        FramedWrite,
    };

    async fn assert_encode_equals(
        input: impl IntoIterator<Item = impl AsRef<[u8]>>,
        expect: impl IntoIterator<Item = u8>,
    ) -> eyre::Result<()> {
        let mut v = vec![];
        let mut writer = FramedWrite::new(&mut v, CobsCodec);

        futures::stream::iter(input.into_iter().map(|x| Ok(x.as_ref().to_vec())))
            .forward(&mut writer)
            .await?;

        let expect = expect.into_iter().collect::<Vec<_>>();
        assert_eq!(expect, v);

        Ok(())
    }

    #[tokio::test]
    async fn encode_test() -> eyre::Result<()> {
        assert_encode_equals(&[&[0]], vec![1, 1, 0]).await?;
        Ok(())
    }

    #[tokio::test]
    async fn encode_test_proptest_1() -> eyre::Result<()> {
        assert_encode_equals(&[&[1], &[1]], vec![2, 1, 0, 2, 1, 0]).await?;
        Ok(())
    }

    #[tokio::test]
    async fn decode_test() -> eyre::Result<()> {
        let input = vec![1, 1];
        let reader = FramedRead::new(&input[..], CobsCodec);

        let mut result = reader.collect::<Vec<Result<Bytes, _>>>().await;

        let first = result.pop().unwrap()?;
        assert_eq!(&[0], first.as_ref());

        Ok(())
    }

    #[tokio::test]
    async fn decode_test_multi() -> eyre::Result<()> {
        let input = vec![1, 1, 0, 1, 1, 0, 0, 0, 0, 1, 4, 1, 2, 3];
        let reader = FramedRead::new(&input[..], CobsCodec);

        let result = reader.collect::<Vec<Result<Bytes, _>>>().await;
        let expect: Vec<Vec<u8>> = vec![vec![0], vec![0], vec![], vec![], vec![], vec![0, 1, 2, 3]];

        assert_eq!(expect.len(), result.len());

        for (expect, result) in expect.into_iter().zip(result) {
            assert_eq!(&expect, result?.as_ref());
        }

        Ok(())
    }

    #[tokio::test]
    async fn decode_test_partial() -> eyre::Result<()> {
        let (r, mut w) = sluice::pipe::pipe();

        let mut reader = FramedRead::new(async_compat::Compat::new(r), CobsCodec);

        // partial message doesn't complete
        w.write_all(&[1, 1]).await?;
        let try_read = tokio::time::timeout(Duration::from_millis(250), reader.next()).await;
        assert!(try_read.is_err(), "got: {:?}", try_read);

        // but once terminated, it does
        w.write_all(&[0]).await?;
        let read = reader.next().await.unwrap()?;
        assert_eq!(&[0], read.as_ref());

        // empty messages are possible
        w.write_all(&[0]).await?;
        let read = reader.next().await.unwrap()?;
        assert_eq!(&[] as &[u8], read.as_ref());

        // invalid string errors
        w.write_all(&[3, 0]).await?;
        let read = reader.next().await.unwrap();
        assert!(read.is_err());
        assert!(reader.next().await.is_none());

        // partial after already decoding doesn't complete
        w.write_all(&[1, 1]).await?;
        let try_read = tokio::time::timeout(Duration::from_millis(250), reader.next()).await;
        assert!(try_read.is_err(), "got: {:?}", try_read);

        // additional partials don't complete
        w.write_all(&[2, 1, 1]).await?;
        let try_read = tokio::time::timeout(Duration::from_millis(250), reader.next()).await;
        assert!(try_read.is_err(), "got: {:?}", try_read);

        // but eventually resolve
        w.write_all(&[0]).await?;
        let read = reader.next().await.unwrap()?;
        assert_eq!(&[0, 0, 1, 0], read.as_ref());

        // EOF reads none
        drop(w);
        let read = reader.next().await;
        assert!(read.is_none(), "got: {:?}", read);

        Ok(())
    }

    proptest! {
        #[test]
        fn test_enc_dec(to_encode in any::<Vec<Vec<u8>>>()) {
            let to_encode = to_encode.into_iter().map(|x| x.into_iter().map(|x| x ^ 0).collect::<Vec<_>>()).collect::<Vec<_>>();

            let (r, w) = sluice::pipe::pipe();

            let reader = FramedRead::new(async_compat::Compat::new(r), CobsCodec);
            let mut writer = FramedWrite::new(async_compat::Compat::new(w), CobsCodec);

            tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(async move {
                let read_task = tokio::spawn(async move {
                    let reader = reader;

                    reader.collect::<Vec<Result<Bytes, _>>>().await
                });

                futures::stream::iter(to_encode.clone())
                    .map(Ok)
                    .forward(&mut writer)
                    .await
                    .unwrap();

                let content: Vec<Result<Bytes, _>> = read_task.await?;
                assert_eq!(to_encode.len(), content.len());

                for (expect, actual) in to_encode.into_iter().zip(content) {
                    assert_eq!(expect, actual?);
                }

                Ok(()) as eyre::Result<()>
            }).unwrap();
        }
    }
}
