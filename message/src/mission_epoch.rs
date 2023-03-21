use chrono::{
    DateTime,
    Utc,
};
use packed_struct::prelude::*;

lazy_static::lazy_static! {
    // TODO: select fixed date
    pub static ref EPOCH: chrono::DateTime<chrono::Utc> = {
        let ndt = chrono::NaiveDate::from_ymd(2022, 6, 1).and_hms(0, 0, 0);

        chrono::DateTime::from_utc(ndt, chrono::Utc)
    };
}

#[derive(
    Copy,
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    PackedStruct,
    serde::Serialize,
    serde::Deserialize,
)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "4", endian = "lsb")]
pub struct MissionEpoch {
    val: u32,
}

impl MissionEpoch {
    #[inline]
    pub const fn new(val: u32) -> Self {
        Self {
            val,
        }
    }

    pub fn now() -> Self {
        Utc::now().into()
    }
}

impl const From<MissionEpoch> for u32 {
    #[inline]
    fn from(val: MissionEpoch) -> Self {
        val.val
    }
}

impl const From<u32> for MissionEpoch {
    #[inline]
    fn from(val: u32) -> Self {
        Self::new(val)
    }
}

impl From<MissionEpoch> for DateTime<Utc> {
    fn from(val: MissionEpoch) -> Self {
        *EPOCH + chrono::Duration::milliseconds(val.val as i64)
    }
}

impl From<DateTime<Utc>> for MissionEpoch {
    fn from(dt: DateTime<Utc>) -> Self {
        let dur = dt - *EPOCH;

        Self {
            val: (dur.num_milliseconds() % u32::MAX as i64) as u32,
        }
    }
}
