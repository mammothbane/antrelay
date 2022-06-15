use std::sync::atomic::{
    AtomicU32,
    Ordering,
};

use antrelay::{
    message::header::Clock,
    MissionEpoch,
};

lazy_static::lazy_static! {
    static ref DUMMY_CLOCK_INSTANCE: DummyClock = DummyClock(AtomicU32::new(0));
}

pub struct DummyClock(AtomicU32);

impl DummyClock {
    pub fn get() -> MissionEpoch {
        MissionEpoch::from(DUMMY_CLOCK_INSTANCE.0.load(Ordering::Acquire))
    }

    pub fn set(val: MissionEpoch) {
        DUMMY_CLOCK_INSTANCE.0.fetch_add(val.into(), Ordering::Release);
    }

    pub fn increment() -> MissionEpoch {
        let atomic: &AtomicU32 = &DUMMY_CLOCK_INSTANCE.0;

        let mut old = atomic.load(Ordering::Acquire);

        loop {
            match atomic.compare_exchange_weak(old, old + 1, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => return old.into(),
                Err(x) => old = x,
            }
        }
    }
}

impl Clock for DummyClock {
    fn now() -> MissionEpoch {
        Self::increment()
    }
}
