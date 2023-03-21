use packed_struct::prelude::*;

use crate::Vec3;

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Hash, PackedStruct,
)]
#[packed_struct(bit_numbering = "msb0", size_bytes = "20", endian = "lsb")]
pub struct Payload {
    pub temperature: u16,
    pub power_5v:    u8,
    pub power_vcc:   u8,
    pub fram_used:   u8,

    #[packed_field(size_bytes = "12")]
    pub accel: Vec3,

    pub heat_enabled: bool,

    #[serde(skip)]
    #[doc(hidden)]
    pub _r0: u16,
}

impl Payload {
    #[inline]
    pub fn display(&self) -> String {
        let &Payload {
            temperature,
            power_5v,
            power_vcc,
            fram_used,
            accel,
            heat_enabled,
            ..
        } = self;

        let heat = if heat_enabled {
            "on"
        } else {
            "off"
        };

        let accel = accel.display();

        format!(
            "temp: {temperature}, {power_5v}v/5v, {power_vcc}/3v, accel: {accel}, heat: {heat}, fram: {fram_used}B"
        )
    }
}
