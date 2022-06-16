use crate::{
    util::Seq,
    MissionEpoch,
};

pub trait Clock {
    fn now(&self) -> MissionEpoch;
}

impl Clock for chrono::Utc {
    #[inline]
    fn now(&self) -> MissionEpoch {
        Self::now().into()
    }
}

impl<T> Clock for T
where
    T: Seq<Output = u32>,
{
    fn now(&self) -> MissionEpoch {
        self.next().into()
    }
}
