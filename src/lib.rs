#![feature(never_type)]
#![feature(try_blocks)]
#![feature(const_option_ext)]
#![feature(option_result_contains)]

pub mod build;
pub mod message;
pub mod util;

lazy_static::lazy_static! {
    // TODO
    pub static ref EPOCH: chrono::DateTime<chrono::Utc> = {
        let ndt = chrono::NaiveDate::from_ymd(2022, 6, 1).and_hms(0, 0, 0);

        chrono::DateTime::from_utc(ndt, chrono::Utc)
    };
}

pub fn from_rel_timestamp(timestamp: u32) -> chrono::DateTime<chrono::Utc> {
    *EPOCH + chrono::Duration::milliseconds(timestamp as i64)
}

pub fn mk_timestamp(datetime: chrono::DateTime<chrono::Utc>) -> u32 {
    let dur = datetime - *EPOCH;
    (dur.num_milliseconds() % u32::MAX as i64) as u32
}

pub fn now() -> u32 {
    mk_timestamp(chrono::Utc::now())
}
