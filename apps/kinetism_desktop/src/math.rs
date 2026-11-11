//! Minimal 3D vector and quaternion math (f64) used by the sensor pipeline.

use std::ops::{Add, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    pub const X: Vec3 = Vec3::new(1.0, 0.0, 0.0);
    pub const Y: Vec3 = Vec3::new(0.0, 1.0, 0.0);
    pub const Z: Vec3 = Vec3::new(0.0, 0.0, 1.0);
    /// Gravity direction in a Z-up world frame.
    pub const DOWN: Vec3 = Vec3::new(0.0, 0.0, -1.0);

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    pub fn scale(self, s: f64) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }

    pub fn normalized(self) -> Option<Vec3> {
        let len = self.length();
        if len.is_finite() && len > 1e-9 {
            Some(self.scale(1.0 / len))
        } else {
            None
        }
    }

    /// Angle between two vectors in radians (0..=PI).
    pub fn angle_to(self, o: Vec3) -> f64 {
        let denom = self.length() * o.length();
        if denom <= 1e-12 {
            return 0.0;
        }
        (self.dot(o) / denom).clamp(-1.0, 1.0).acos()
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

/// Unit quaternion (w + xi + yj + zk) representing a sensor-to-world rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Default for Quat {
    fn default() -> Self {
        Quat::IDENTITY
    }
}

impl Quat {
    pub const IDENTITY: Quat = Quat {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub const fn new(w: f64, x: f64, y: f64, z: f64) -> Self {
        Self { w, x, y, z }
    }

    pub fn norm(self) -> f64 {
        (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn normalized(self) -> Option<Quat> {
        let n = self.norm();
        if n.is_finite() && n > 1e-6 {
            Some(Quat::new(self.w / n, self.x / n, self.y / n, self.z / n))
        } else {
            None
        }
    }

    pub fn conjugate(self) -> Quat {
        Quat::new(self.w, -self.x, -self.y, -self.z)
    }

    /// Rotation of `angle` radians about `axis`.
    pub fn from_axis_angle(axis: Vec3, angle: f64) -> Quat {
        let a = axis.normalized().unwrap_or(Vec3::Z);
        let (s, c) = (angle * 0.5).sin_cos();
        Quat::new(c, a.x * s, a.y * s, a.z * s)
    }

    /// Shortest-arc rotation that maps unit vector `from` onto unit vector `to`.
    pub fn from_two_vectors(from: Vec3, to: Vec3) -> Quat {
        let f = from.normalized().unwrap_or(Vec3::X);
        let t = to.normalized().unwrap_or(Vec3::X);
        let d = f.dot(t);
        if d >= 1.0 - 1e-12 {
            return Quat::IDENTITY;
        }
        if d <= -1.0 + 1e-12 {
            let mut axis = Vec3::X.cross(f);
            if axis.length() < 1e-6 {
                axis = Vec3::Y.cross(f);
            }
            return Quat::from_axis_angle(axis, std::f64::consts::PI);
        }
        let c = f.cross(t);
        Quat::new(1.0 + d, c.x, c.y, c.z)
            .normalized()
            .unwrap_or(Quat::IDENTITY)
    }

    /// Rotate a vector by this (unit) quaternion.
    pub fn rotate(self, v: Vec3) -> Vec3 {
        let u = Vec3::new(self.x, self.y, self.z);
        let uv = u.cross(v);
        let uuv = u.cross(uv);
        v + (uv.scale(self.w) + uuv).scale(2.0)
    }
}

impl Mul for Quat {
    type Output = Quat;
    fn mul(self, o: Quat) -> Quat {
        Quat::new(
            self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        )
    }
}

/// Rotate a vector about the world Z (up) axis.
pub fn rotate_about_z(v: Vec3, angle: f64) -> Vec3 {
    let (s, c) = angle.sin_cos();
    Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z)
}

/// Heading (radians) of the horizontal projection of `v`, if it is not vertical.
pub fn horizontal_heading(v: Vec3, min_horizontal: f64) -> Option<f64> {
    let h = (v.x * v.x + v.y * v.y).sqrt();
    if h < min_horizontal {
        None
    } else {
        Some(v.y.atan2(v.x))
    }
}

/// Wrap an angle to (-PI, PI].
pub fn wrap_angle(a: f64) -> f64 {
    let two_pi = std::f64::consts::TAU;
    let mut r = a % two_pi;
    if r <= -std::f64::consts::PI {
        r += two_pi;
    } else if r > std::f64::consts::PI {
        r -= two_pi;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-9
    }

    #[test]
    fn rotates_about_axis() {
        let q = Quat::from_axis_angle(Vec3::Z, FRAC_PI_2);
        assert!(close(q.rotate(Vec3::X), Vec3::Y));
    }

    #[test]
    fn composes_rotations_in_order() {
        let a = Quat::from_axis_angle(Vec3::Z, FRAC_PI_2);
        let b = Quat::from_axis_angle(Vec3::X, FRAC_PI_2);
        // (a*b).rotate(v) == a.rotate(b.rotate(v))
        let v = Vec3::new(0.3, -0.2, 0.9);
        assert!(close((a * b).rotate(v), a.rotate(b.rotate(v))));
    }

    #[test]
    fn two_vector_rotation_maps_exactly() {
        let from = Vec3::new(1.0, 2.0, -0.5).normalized().unwrap();
        let to = Vec3::new(-0.3, 0.1, 0.8).normalized().unwrap();
        assert!(close(Quat::from_two_vectors(from, to).rotate(from), to));
        assert!(close(
            Quat::from_two_vectors(from, -from).rotate(from),
            -from
        ));
    }

    #[test]
    fn wraps_angles() {
        assert!((wrap_angle(3.0 * std::f64::consts::PI) - std::f64::consts::PI).abs() < 1e-9);
        assert!((wrap_angle(-FRAC_PI_2 - std::f64::consts::TAU) + FRAC_PI_2).abs() < 1e-9);
    }
}
