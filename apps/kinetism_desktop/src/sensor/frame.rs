//! Parser for the `Physio_A` BLE notification payload.
//!
//! The master sketch sends exactly eight comma-separated floats: the upper-arm
//! (master) quaternion followed by the forearm (slave) quaternion, each in
//! `x,y,z,w` order. A sensor that has not reported yet is sent as all zeros,
//! which is treated as "missing" rather than as an invalid packet so shoulder
//! measurements keep working when only the upper-arm sensor is streaming.

use crate::math::Quat;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SensorFrame {
    pub upper: Option<Quat>,
    pub forearm: Option<Quat>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    NotUtf8,
    WrongFieldCount(usize),
    NotANumber(usize),
    NoOrientation,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::NotUtf8 => write!(f, "packet is not UTF-8 text"),
            FrameError::WrongFieldCount(n) => write!(f, "expected 8 values, got {n}"),
            FrameError::NotANumber(i) => write!(f, "value {} is not a finite number", i + 1),
            FrameError::NoOrientation => write!(f, "neither sensor reported an orientation"),
        }
    }
}

impl std::error::Error for FrameError {}

fn quat_from_xyzw(v: &[f64]) -> Option<Quat> {
    // A zero quaternion means the sensor has not reported (firmware default).
    Quat::new(v[3], v[0], v[1], v[2]).normalized()
}

pub fn parse_frame(bytes: &[u8]) -> Result<SensorFrame, FrameError> {
    let text = std::str::from_utf8(bytes).map_err(|_| FrameError::NotUtf8)?;
    let text = text.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() != 8 {
        return Err(FrameError::WrongFieldCount(parts.len()));
    }
    let mut values = [0.0f64; 8];
    for (i, part) in parts.iter().enumerate() {
        let value: f64 = part.trim().parse().map_err(|_| FrameError::NotANumber(i))?;
        if !value.is_finite() {
            return Err(FrameError::NotANumber(i));
        }
        values[i] = value;
    }
    let upper = quat_from_xyzw(&values[0..4]);
    let forearm = quat_from_xyzw(&values[4..8]);
    if upper.is_none() && forearm.is_none() {
        return Err(FrameError::NoOrientation);
    }
    Ok(SensorFrame { upper, forearm })
}

/// Format a frame back into the firmware's wire format (used by the simulator).
pub fn encode_frame(upper: Quat, forearm: Option<Quat>) -> String {
    let f = forearm.unwrap_or(Quat::new(0.0, 0.0, 0.0, 0.0));
    format!(
        "{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}",
        upper.x, upper.y, upper.z, upper.w, f.x, f.y, f.z, f.w
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_upper_then_forearm_in_xyzw_order() {
        let frame = parse_frame(b"0.000,0.000,0.707,0.707,1.0,0,0,0").unwrap();
        let upper = frame.upper.unwrap();
        let half = std::f64::consts::FRAC_1_SQRT_2;
        assert!((upper.w - half).abs() < 1e-3 && (upper.z - half).abs() < 1e-3);
        let fore = frame.forearm.unwrap();
        assert!((fore.x - 1.0).abs() < 1e-9 && fore.w.abs() < 1e-9);
    }

    #[test]
    fn missing_forearm_is_not_an_error() {
        let frame = parse_frame(b"0,0,0,1,0.000,0.000,0.000,0.000\0").unwrap();
        assert!(frame.upper.is_some());
        assert!(frame.forearm.is_none());
    }

    #[test]
    fn rejects_malformed_packets() {
        assert_eq!(parse_frame(b"1,2,3"), Err(FrameError::WrongFieldCount(3)));
        assert_eq!(
            parse_frame(b"0,0,0,1,0,0,0,x"),
            Err(FrameError::NotANumber(7))
        );
        assert_eq!(
            parse_frame(b"0,0,0,0,0,0,0,0"),
            Err(FrameError::NoOrientation)
        );
        assert_eq!(
            parse_frame(b"0,0,0,inf,0,0,0,1"),
            Err(FrameError::NotANumber(3))
        );
        assert_eq!(parse_frame(&[0xff, 0xfe]), Err(FrameError::NotUtf8));
    }

    #[test]
    fn invalid_packet_reasons_read_as_plain_sentences() {
        // Each reason is kept as `LinkStats::last_invalid` for diagnosing a bad link.
        let reason = |bytes: &[u8]| parse_frame(bytes).unwrap_err().to_string();
        assert_eq!(reason(&[0xff, 0xfe]), "packet is not UTF-8 text");
        assert_eq!(reason(b"1,2,3"), "expected 8 values, got 3");
        assert_eq!(
            reason(b"0,0,0,1,0,0,0,x"),
            "value 8 is not a finite number",
            "values are counted from 1 for people reading the message"
        );
        assert_eq!(
            reason(b"nan,0,0,1,0,0,0,1"),
            "value 1 is not a finite number"
        );
        assert_eq!(
            reason(b"0,0,0,0,0,0,0,0"),
            "neither sensor reported an orientation"
        );
    }

    #[test]
    fn round_trips_simulator_encoding() {
        let q = Quat::from_axis_angle(crate::math::Vec3::new(0.2, 0.5, 0.8), 1.1);
        let frame = parse_frame(encode_frame(q, None).as_bytes()).unwrap();
        let u = frame.upper.unwrap();
        assert!((u.w - q.w).abs() < 2e-3 && (u.y - q.y).abs() < 2e-3);
    }
}
