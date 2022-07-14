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
        let result = src.split_to(report.src_used);

        Ok(Some(result.into()))
    }
}
