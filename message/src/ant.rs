use packed_struct::prelude::*;

use crate::Vec3;

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Hash, PackedStruct,
)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "52", endian = "lsb")]
pub struct Payload {
    pub pcb_temp:        u16,
    pub battery_temp:    u16,
    pub battery_voltage: u8,
    pub fram_usage:      u16,

    #[packed_field(size_bytes = "12")]
    pub gyro: Vec3,

    #[packed_field(size_bytes = "12")]
    pub accelerometer: Vec3,

    pub calipile_object:  u32,
    pub calipile_ambient: u32,

    pub orientation: u16,
    pub steer:       u8,

    #[doc(hidden)]
    #[serde(skip)]
    pub _r0: u64,

    #[doc(hidden)]
    #[serde(skip)]
    pub _r1: u16,
}

impl Payload {
    #[inline]
    pub fn display(&self) -> String {
        let &Payload {
            pcb_temp,
            battery_temp,
            battery_voltage,
            gyro,
            accelerometer,
            calipile_object,
            calipile_ambient,
            orientation,
            steer,
            ..
        } = self;

        let accel = accelerometer.display();
        let gyro = gyro.display();

        format!(
            "tp: {calipile_object} obj / {calipile_ambient} amb, accel: {accel}, gyro: {gyro}, steer: {steer}, orient: {orientation}, pcb: {pcb_temp}°C, bat: temp {battery_temp}, {battery_voltage}V",
        )
    }
}
