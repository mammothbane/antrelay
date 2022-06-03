#![allow(unused)]

// Everything in this module pulled directly from futures-rs.

use futures::{
    Future,
    Stream,
};

mod scan;
mod unfold_state;

pub use scan::Scan;

pub trait StreamExt: Stream {
    fn scan<S, B, Fut, F>(self, initial_state: S, f: F) -> Scan<Self, S, Fut, F>
    where
        F: FnMut(S, Self::Item) -> Fut,
        Fut: Future<Output = Option<(S, B)>>,
        Self: Sized,
    {
        assert_stream::<B, _>(Scan::new(self, initial_state, f))
    }

    fn owned_scan<S, B, Fut, F>(self, initial_state: S, f: F) -> Scan<Self, S, Fut, F>
    where
        F: FnMut(S, Self::Item) -> Fut,
        Fut: Future<Output = Option<(S, B)>>,
        Self: Sized,
    {
        assert_stream::<B, _>(Scan::new(self, initial_state, f))
    }
}

impl<T> StreamExt for T where T: Stream {}

// Just a helper function to ensure the streams we're returning all have the
// right implementations.
pub(crate) fn assert_stream<T, S>(stream: S) -> S
where
    S: Stream<Item = T>,
{
    stream
}
