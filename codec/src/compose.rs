use std::marker::PhantomData;

use bytes::BytesMut;
use tokio_util::codec::{
    Decoder,
    Encoder,
};

pub struct ComposeCodecs<T, U, E>(T, U, PhantomData<E>);

impl<T, U, E> ComposeCodecs<T, U, E> {
    pub fn new(t: T, u: U) -> Self {
        Self(t, u, PhantomData)
    }
}

impl<T, U, V, Item, E> Encoder<U> for ComposeCodecs<T, U, E>
where
    T: Encoder<Item>,
    U: Encoder<V>,
    V: AsRef<[u8]>,
    E: From<U::Error> + From<T::Error>,
{
    type Error = E;

    fn encode(&mut self, item: Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let mut b = BytesMut::new();

        self.0.encode(item, &mut b)?;
        self.1.encode(b, dst)?;

        Ok(())
    }
}

impl<T, U, E> Decoder for ComposeCodecs<T, U, E>
where
    T: Decoder,
    U: Decoder,
    U::Item: Into<BytesMut>,
    E: From<U::Error> + From<T::Error>,
{
    type Error = E;
    type Item = T::Item;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let result = self.1.decode(src)?;

        let result = match result {
            None => None,
            Some(item) => self.0.decode(&mut item.into())?,
        };

        Ok(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test() -> eyre::Result<()> {
        let mut compose = ComposeCodecs::new(
            tokio_util::codec::AnyDelimiterCodec::new(vec![0], vec![0]),
            tokio_util::codec::LinesCodec::new(),
        );
        let mut dst = BytesMut::new();

        compose.encode(vec![1, 2, 3], &mut dst)?;
        assert_eq!(dst.as_ref(), &[1, 2, 3, 0, b'\n']);

        Ok(())
    }
}
