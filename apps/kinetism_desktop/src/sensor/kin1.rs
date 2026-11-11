//! `KIN1` Wi-Fi telemetry packets from the root PlatformIO firmware
//! (layout in `include/telemetry_protocol.h`, 64 bytes, little-endian).

use crate::math::Quat;

pub const MAGIC: u32 = 0x314E_494B;
pub const VERSION: u8 = 1;
pub const SIZE: usize = 64;
pub const FLAG_SENSOR_OK: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kin1Packet {
    pub flags: u8,
    pub sequence: u32,
    pub timestamp_ms: u32,
    /// Game rotation vector (gravity-referenced tilt, arbitrary heading).
    pub quaternion: Quat,
    pub battery_percent: u8,
    pub accuracy: u8,
    pub sample_rate_hz: f32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Kin1Error {
    WrongSize(usize),
    BadMagic(u32),
    BadVersion(u8),
    BadDeclaredSize(u16),
    ReservedNotZero,
    NonFinite,
    BadBattery(u8),
    SensorNotOk,
    BadQuaternion,
}

impl std::fmt::Display for Kin1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Kin1Error::WrongSize(n) => write!(f, "expected {SIZE} bytes, got {n}"),
            Kin1Error::BadMagic(m) => write!(f, "invalid magic 0x{m:08x}"),
            Kin1Error::BadVersion(v) => write!(f, "unsupported protocol version {v}"),
            Kin1Error::BadDeclaredSize(s) => write!(f, "declared packet size {s} is invalid"),
            Kin1Error::ReservedNotZero => write!(f, "reserved field must be zero"),
            Kin1Error::NonFinite => write!(f, "packet contains a non-finite value"),
            Kin1Error::BadBattery(b) => write!(f, "invalid battery percentage {b}"),
            Kin1Error::SensorNotOk => write!(f, "sensor reports its IMU is not ready"),
            Kin1Error::BadQuaternion => write!(f, "quaternion is not unit length"),
        }
    }
}

impl std::error::Error for Kin1Error {}

fn u16_at(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}

fn u32_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn f32_at(d: &[u8], o: usize) -> f32 {
    f32::from_bits(u32_at(d, o))
}

/// Same validation as `pc/app/protocol.py`, plus the firmware's sensor-OK flag.
pub fn parse(d: &[u8]) -> Result<Kin1Packet, Kin1Error> {
    if d.len() != SIZE {
        return Err(Kin1Error::WrongSize(d.len()));
    }
    let magic = u32_at(d, 0);
    if magic != MAGIC {
        return Err(Kin1Error::BadMagic(magic));
    }
    if d[4] != VERSION {
        return Err(Kin1Error::BadVersion(d[4]));
    }
    let declared = u16_at(d, 6);
    if usize::from(declared) != SIZE {
        return Err(Kin1Error::BadDeclaredSize(declared));
    }
    if u16_at(d, 58) != 0 {
        return Err(Kin1Error::ReservedNotZero);
    }
    let values: [f32; 10] = std::array::from_fn(|i| f32_at(d, 16 + 4 * i));
    let sample_rate_hz = f32_at(d, 60);
    if !values
        .iter()
        .chain([&sample_rate_hz])
        .all(|v| v.is_finite())
    {
        return Err(Kin1Error::NonFinite);
    }
    let battery_percent = d[56];
    if battery_percent > 100 {
        return Err(Kin1Error::BadBattery(battery_percent));
    }
    let flags = d[5];
    if flags & FLAG_SENSOR_OK == 0 {
        return Err(Kin1Error::SensorNotOk);
    }
    let raw = Quat::new(
        f64::from(values[0]),
        f64::from(values[1]),
        f64::from(values[2]),
        f64::from(values[3]),
    );
    if !(0.8..=1.2).contains(&raw.norm()) {
        return Err(Kin1Error::BadQuaternion);
    }
    Ok(Kin1Packet {
        flags,
        sequence: u32_at(d, 8),
        timestamp_ms: u32_at(d, 12),
        quaternion: raw.normalized().ok_or(Kin1Error::BadQuaternion)?,
        battery_percent,
        accuracy: d[57],
        sample_rate_hz,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from the live Wi-Fi sensor (firmware in `src/main.cpp`).
    const LIVE_PACKET: &str = "4b494e3101074000c2160000d3ea00000000083d0000f03a00987fbf00803cbd0000e03c0000e0bc000080bc000000000000000000000000640300000000c842";

    fn live() -> Vec<u8> {
        (0..LIVE_PACKET.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&LIVE_PACKET[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn decodes_a_real_firmware_packet() {
        let p = parse(&live()).unwrap();
        assert_eq!(p.flags, 7);
        assert_eq!(p.sequence, 5826);
        assert_eq!(p.timestamp_ms, 60115);
        assert_eq!(p.battery_percent, 100);
        assert_eq!(p.accuracy, 3);
        assert_eq!(p.sample_rate_hz, 100.0);
        assert!((p.quaternion.w - 0.0332).abs() < 1e-3);
        assert!((p.quaternion.y + 0.9984).abs() < 1e-3);
        assert!((p.quaternion.norm() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_invalid_packets() {
        let good = live();
        let with = |offset: usize, bytes: &[u8]| {
            let mut d = good.clone();
            d[offset..offset + bytes.len()].copy_from_slice(bytes);
            d
        };
        assert_eq!(parse(&good[..63]), Err(Kin1Error::WrongSize(63)));
        assert!(matches!(
            parse(&with(0, b"KIN2")),
            Err(Kin1Error::BadMagic(_))
        ));
        assert_eq!(parse(&with(4, &[2])), Err(Kin1Error::BadVersion(2)));
        assert_eq!(
            parse(&with(6, &[60, 0])),
            Err(Kin1Error::BadDeclaredSize(60))
        );
        assert_eq!(parse(&with(58, &[1, 0])), Err(Kin1Error::ReservedNotZero));
        assert_eq!(
            parse(&with(16, &f32::NAN.to_le_bytes())),
            Err(Kin1Error::NonFinite)
        );
        assert_eq!(parse(&with(56, &[101])), Err(Kin1Error::BadBattery(101)));
        assert_eq!(parse(&with(5, &[6])), Err(Kin1Error::SensorNotOk));
        assert_eq!(
            parse(&with(16, &2.0f32.to_le_bytes())),
            Err(Kin1Error::BadQuaternion)
        );
    }

    #[test]
    fn rejected_packet_reasons_name_the_bad_field_and_value() {
        // Each reason is kept as `LinkStats::last_invalid` for diagnosing a bad link.
        let good = live();
        let reason = |offset: usize, bytes: &[u8]| {
            let mut d = good.clone();
            d[offset..offset + bytes.len()].copy_from_slice(bytes);
            parse(&d).unwrap_err().to_string()
        };
        assert_eq!(
            parse(&good[..63]).unwrap_err().to_string(),
            "expected 64 bytes, got 63"
        );
        assert_eq!(reason(0, b"KIN2"), "invalid magic 0x324e494b");
        assert_eq!(
            reason(0, &[0xab, 0, 0, 0]),
            "invalid magic 0x000000ab",
            "the magic is shown as zero-padded hex"
        );
        assert_eq!(reason(4, &[2]), "unsupported protocol version 2");
        assert_eq!(reason(6, &[60, 0]), "declared packet size 60 is invalid");
        assert_eq!(reason(58, &[1, 0]), "reserved field must be zero");
        assert_eq!(
            reason(60, &f32::INFINITY.to_le_bytes()),
            "packet contains a non-finite value",
            "a broken sample rate is rejected like a broken quaternion"
        );
        assert_eq!(reason(56, &[101]), "invalid battery percentage 101");
        assert_eq!(reason(5, &[6]), "sensor reports its IMU is not ready");
        assert_eq!(
            reason(16, &2.0f32.to_le_bytes()),
            "quaternion is not unit length"
        );
    }
}
