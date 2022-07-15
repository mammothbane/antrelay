use aho_corasick::{
    AhoCorasick,
    AhoCorasickBuilder,
    MatchKind,
};
use bytes::{
    Buf,
    BufMut,
    Bytes,
    BytesMut,
};
use tokio_util::codec::{
    Decoder,
    Encoder,
};

/// A codec that captures a single sequence of delimiters.
/// It can operate in a mode where the delimiter is in fact part of the message ("inclusive" mode),
/// or in which the messages are separated by the delimiter, which is not part of the output
/// ("exclusive" mode).
///
/// This is a stateful codec.
///
/// ## Inclusive mode
/// On start, the codec discards bytes until it reads the delimiter. Everything after the start of
/// the delimiter, up to the start of the next one, is the message. EOF will also terminate a
/// message.
///
/// The encoder expects each incoming message to start with the delimiter and will error if it is
/// not present.
///
/// ## Exclusive mode
/// Messages are strictly delimiter-terminated in this mode. Note that this means an EOF will *not*
/// terminate a message -- each message must end with a delimiter. The message stream may start with
/// a partial message if the codec started decoding in the middle of one.
#[derive(Debug)]
pub struct AllDelimiterCodec {
    inclusive: bool,
    delimiter: Bytes,

    ac: AhoCorasick,

    inclusive_seen_header: bool,
    inclusive_terminated:  bool,
    search_from:           usize,
}

impl AllDelimiterCodec {
    pub fn new(delimiter: impl AsRef<[u8]>, inclusive: bool) -> Self {
        let delimiter = delimiter.as_ref();

        let ac = AhoCorasickBuilder::new()
            .match_kind(MatchKind::LeftmostLongest)
            .build((1..delimiter.len() + 1).map(|i| &delimiter[..i]));

        Self {
            inclusive,
            delimiter: Bytes::copy_from_slice(delimiter),
            ac,
            inclusive_seen_header: false,
            inclusive_terminated: false,
            search_from: 0,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("inclusive mode was specified, but provided item did not start with delimiter")]
    InclusiveMismatch,

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl AllDelimiterCodec {
    fn make_decoded(&self, b: BytesMut) -> Bytes {
        if !self.inclusive {
            return b.freeze();
        }

        let rlen = b.len();
        self.delimiter.clone().chain(b).copy_to_bytes(self.delimiter.len() + rlen)
    }
}

impl<T> Encoder<T> for &AllDelimiterCodec
where
    T: AsRef<[u8]>,
{
    type Error = Error;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let item = item.as_ref();

        if self.inclusive && !item.starts_with(self.delimiter.as_ref()) {
            return Err(Error::InclusiveMismatch);
        }

        let len = if self.inclusive {
            item.len()
        } else {
            item.len() + self.delimiter.len()
        };

        dst.reserve(len);
        dst.put(item);

        if !self.inclusive {
            dst.put(self.delimiter.as_ref());
        }

        Ok(())
    }
}

impl Decoder for AllDelimiterCodec {
    type Error = Error;
    type Item = Bytes;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        eprintln!("hello? {:?}", src);

        // loop serves 2 functions:
        //      - aho_corasick finds the leftmost match, even a partial one. with delim [0x0, 0x0],
        //        the string [0x1, 0x0, 0x1, 0x0, 0x0, 0x1, 0x0, 0x0] will result in a partial match
        //        at position 1 first. note that there are two complete packets here. we want to
        //        capture the first one, so we can't search backwards from the end of the string.
        //        hence:
        //          - if the match is complete, capture and return the packet (regardless of where
        //              it occurs)
        //          - if the match is partial and ends at the end of the string, stop looking
        //          - if the match is partial and does not end at the end of the string, keep
        //              looking, starting at the next position in the string after the start of the
        //              match
        //          - if no match, stop looking

        loop {
            return match self.ac.find(&src[self.search_from..]) {
                Some(mat) if mat.end() - mat.start() == self.delimiter.len() => {
                    let mut result = src.split_to(self.search_from + mat.end());
                    self.search_from = 0;

                    result.truncate(result.len() - self.delimiter.len());

                    if self.inclusive && !self.inclusive_seen_header {
                        self.inclusive_seen_header = true;
                        continue;
                    }

                    Ok(Some(self.make_decoded(result)))
                },

                Some(mat) if self.search_from + mat.end() == src.len() => {
                    self.search_from += mat.start();
                    Ok(None)
                },

                Some(mat) => {
                    self.search_from += mat.start() + 1;
                    continue;
                },

                None => {
                    self.search_from = src.len();
                    Ok(None)
                },
            };
        }
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if !self.inclusive {
            return self.decode(buf);
        }

        if buf.len() == 0 {
            return Ok(if self.inclusive_seen_header && !self.inclusive_terminated {
                self.inclusive_terminated = true;
                Some(self.make_decoded(BytesMut::new()))
            } else {
                None
            });
        }

        if let result @ Some(_) = self.decode(buf)? {
            return Ok(result);
        }

        self.inclusive_terminated = true;
        self.search_from = 0;
        Ok(Some(self.make_decoded(buf.split())))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::prelude::*;
    use itertools::Itertools;
    use proptest::prelude::*;
    use tokio_util::codec::FramedRead;

    async fn assert_decode<D, V>(
        decode: D,
        src: impl AsRef<[u8]>,
        expect: impl IntoIterator<Item = V>,
    ) -> eyre::Result<()>
    where
        D: Decoder<Item = Bytes>,
        eyre::Report: From<D::Error>,
        V: AsRef<[u8]>,
    {
        let src = src.as_ref().to_vec();

        let results = FramedRead::new(&src[..], decode)
            .map(|e| e.map(|x| x.to_vec()).map_err(eyre::Report::from))
            .collect::<Vec<_>>()
            .await;

        let expect = expect.into_iter().collect::<Vec<_>>();

        assert_eq!(expect.len(), results.len());

        for (got, expect) in results.into_iter().zip(expect) {
            assert_eq!(expect.as_ref(), &got?[..]);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_exclusive_decode() -> eyre::Result<()> {
        assert_decode(AllDelimiterCodec::new(&[0], false), vec![0, 1, 2, 3, 0, 4, 5, 6], vec![
            vec![],
            vec![1, 2, 3],
        ])
        .await
    }

    #[tokio::test]
    async fn test_inclusive_decode() -> eyre::Result<()> {
        assert_decode(AllDelimiterCodec::new(&[0], true), vec![0, 1, 2, 3, 0, 4, 5, 6], vec![
            vec![0, 1, 2, 3],
            vec![0, 4, 5, 6],
        ])
        .await
    }

    #[tokio::test]
    async fn test_exclusive_decode_multi() -> eyre::Result<()> {
        assert_decode(
            AllDelimiterCodec::new(&[0, 1], false),
            vec![0, 1, 1, 2, 3, 0, 1, 4, 5, 6],
            vec![vec![], vec![1, 2, 3]],
        )
        .await
    }

    #[tokio::test]
    async fn test_inclusive_decode_multi() -> eyre::Result<()> {
        assert_decode(AllDelimiterCodec::new(&[0, 1], true), vec![0, 1, 2, 3, 0, 1, 4, 5, 6], vec![
            vec![0, 1, 2, 3],
            vec![0, 1, 4, 5, 6],
        ])
        .await
    }

    #[tokio::test]
    async fn test_exclusive_decode_multi_proptest_1() -> eyre::Result<()> {
        assert_decode(AllDelimiterCodec::new(&[0, 1], false), vec![0, 0, 1], vec![vec![0]]).await
    }

    #[tokio::test]
    async fn test_inclusive_decode_multi_proptest_1() -> eyre::Result<()> {
        assert_decode(AllDelimiterCodec::new(&[0, 1], true), vec![0, 1], vec![vec![0, 1]]).await
    }

    #[tokio::test]
    async fn test_decode_proptest_repeated() -> eyre::Result<()> {
        assert_decode(AllDelimiterCodec::new(&[12, 12], false), vec![12], vec![] as Vec<Vec<u8>>)
            .await
    }

    #[tokio::test]
    async fn test_inclusive_decode_proptest_empty() -> eyre::Result<()> {
        assert_decode(AllDelimiterCodec::new(&[0, 1], true), vec![0, 1], vec![vec![0, 1]]).await
    }

    proptest! {
        #[test]
        fn test_exclusive_prop(delim in any::<Vec<u8>>(), expect in any::<Vec<Vec<u8>>>()) {
            prop_assume!(delim.len() != 0);
            prop_assume!(expect.iter().all(|x| x.windows(delim.len()).all(|w| w != delim)));

            let test = if expect.len() == 0 {
                vec![]
            } else {
                expect.clone().into_iter().interleave_shortest(std::iter::repeat(delim.clone())).flatten().collect::<Vec<u8>>()
            };

            tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(async move {
                assert_decode(
                    AllDelimiterCodec::new(&delim, false),
                    test,
                    expect,
                )
                .await
            }).unwrap();
        }

        #[test]
        fn test_inclusive_prop(delim in any::<Vec<u8>>(), expect in any::<Vec<Vec<u8>>>()) {
            prop_assume!(delim.len() != 0);
            prop_assume!(expect.iter().all(|x| x.windows(delim.len()).all(|w| !delim.starts_with(w))));

            let chunks = std::iter::repeat(delim.clone()).interleave_shortest(expect.clone().into_iter()).chunks(2);

            let expected = chunks.into_iter().map(|pair| pair.flatten().collect::<Vec<_>>()).collect::<Vec<_>>();
            let test = expected.clone().into_iter().flatten().collect::<Vec<u8>>();

            tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(async move {
                assert_decode(
                    AllDelimiterCodec::new(&delim, true),
                    test,
                    expected,
                )
                .await
            }).unwrap();
        }
    }
}
