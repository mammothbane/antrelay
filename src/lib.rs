#![feature(never_type)]
#![feature(try_blocks)]
#![feature(const_option_ext)]
#![feature(adt_const_params)]
#![feature(option_result_contains)]
#![feature(inherent_associated_types)]

pub mod build;
pub mod message;
pub mod util;

lazy_static::lazy_static! {
    // TODO: define
    pub static ref EPOCH: chrono::DateTime<chrono::Utc> = {
        let ndt = chrono::NaiveDate::from_ymd(2022, 06, 01).and_hms(0, 0, 0);

        chrono::DateTime::from_utc(ndt, chrono::Utc)
    };
}
