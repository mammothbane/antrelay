use crate::{
    util::Seq,
    MissionEpoch,
};

pub trait Clock {
    fn now() -> MissionEpoch;
}

impl Clock for chrono::Utc {
    #[inline]
    fn now() -> MissionEpoch {
        Self::now().into()
    }
}

impl<T> Clock for T
where
    T: Seq<Output = u32>,
{
    fn now() -> MissionEpoch {
        <Self as Seq>::next().into()
    }
}
