//! Arm kinematics from the two BNO080 game-rotation-vector quaternions.
//!
//! Each sensor reports its orientation in its own gravity-aligned world frame
//! (Z up). Tilt is shared, but the yaw reference of each sensor is arbitrary
//! and the sensor-to-segment mounting is unknown. A two-pose calibration
//! resolves both:
//!
//! 1. **Rest** - arm relaxed at the side, elbow straight: the gravity vector
//!    seen by each sensor gives the segment's long axis in sensor coordinates.
//! 2. **Forward** - arm raised forward (>= 25 deg), elbow straight: both segments
//!    point the same horizontal way, which gives the patient's forward heading
//!    and the yaw offset between the two sensors' world frames.
//!
//! Body frame used everywhere after calibration: x = patient's right,
//! y = forward, z = up. The game screen is a mirror (patient's right = screen
//! right), so the hand's frontal-plane projection is simply `(x, z)`.

use crate::clinical::Side;
use crate::math::{Quat, Vec3, horizontal_heading, rotate_about_z, wrap_angle};
use std::f64::consts::FRAC_PI_2;

/// Upper-arm share of the acromion-to-wrist length (forearm gets the rest).
pub const UPPER_SHARE: f64 = 0.55;

/// Minimum elevation (deg) before the plane of elevation is meaningful.
pub const MIN_ELEVATION_FOR_PLANE: f64 = 15.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Calibration {
    /// Upper-arm distal axis expressed in the upper sensor's frame.
    pub upper_axis: Vec3,
    /// Forearm distal axis expressed in the forearm sensor's frame.
    pub forearm_axis: Option<Vec3>,
    /// Patient's forward heading in the upper sensor's world frame (rad).
    pub forward_heading: f64,
    /// Yaw that maps forearm-sensor world vectors into the upper-sensor world frame (rad).
    pub forearm_yaw_offset: f64,
    /// `false` when the forward pose was skipped and the heading is estimated.
    pub heading_measured: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArmPose {
    pub upper_dir: Vec3,
    pub forearm_dir: Option<Vec3>,
    /// Upper-arm angle from hanging straight down: 0 at rest, 90 horizontal, 180 overhead.
    pub elevation_deg: f64,
    /// Plane of elevation: 0 = frontal (abduction), +90 = forward (flexion), -90 = backward.
    pub plane_deg: Option<f64>,
    /// 0 = elbow straight.
    pub elbow_flexion_deg: Option<f64>,
    /// Frontal-plane projection of the hand in arm lengths, shoulder at origin.
    pub hand: [f64; 2],
    /// Frontal-plane projection of the elbow in arm lengths.
    pub elbow: [f64; 2],
}

impl Calibration {
    fn to_body(self, world: Vec3) -> Vec3 {
        rotate_about_z(world, FRAC_PI_2 - self.forward_heading)
    }

    pub fn pose(&self, upper: Quat, forearm: Option<Quat>, side: Side) -> ArmPose {
        let upper_dir = self
            .to_body(upper.rotate(self.upper_axis))
            .normalized()
            .unwrap_or(Vec3::DOWN);
        let forearm_dir = match (forearm, self.forearm_axis) {
            (Some(q), Some(axis)) => self
                .to_body(rotate_about_z(q.rotate(axis), self.forearm_yaw_offset))
                .normalized(),
            _ => None,
        };
        let elevation_deg = upper_dir.angle_to(Vec3::DOWN).to_degrees();
        let plane_deg = (elevation_deg >= MIN_ELEVATION_FOR_PLANE).then(|| {
            let lateral = side.sign() * upper_dir.x;
            upper_dir.y.atan2(lateral).to_degrees()
        });
        let elbow_flexion_deg = forearm_dir.map(|f| upper_dir.angle_to(f).to_degrees());
        let elbow_point = upper_dir.scale(UPPER_SHARE);
        let hand_point = elbow_point + forearm_dir.unwrap_or(upper_dir).scale(1.0 - UPPER_SHARE);
        ArmPose {
            upper_dir,
            forearm_dir,
            elevation_deg,
            plane_deg,
            elbow_flexion_deg,
            hand: [hand_point.x, hand_point.z],
            elbow: [elbow_point.x, elbow_point.z],
        }
    }
}

/// Collects samples while the patient holds a calibration pose still.
#[derive(Clone, Debug, Default)]
pub struct PoseAccumulator {
    upper: Vec<Quat>,
    forearm: Vec<Quat>,
}

pub const SAMPLES_PER_POSE: usize = 20;
/// Maximum spread (deg) of the segment axis while "holding still".
pub const STILLNESS_LIMIT_DEG: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AccumulateStatus {
    Collecting(f32),
    Moving,
    Done,
}

impl PoseAccumulator {
    pub fn clear(&mut self) {
        self.upper.clear();
        self.forearm.clear();
    }

    pub fn progress(&self) -> f32 {
        (self.upper.len() as f32 / SAMPLES_PER_POSE as f32).min(1.0)
    }

    /// Push a sample; restarts automatically if the arm moves.
    pub fn push(&mut self, upper: Quat, forearm: Option<Quat>) -> AccumulateStatus {
        if let Some(first) = self.upper.first() {
            // Compare gravity as seen by the sensor: spread means the arm moved.
            let a = first.conjugate().rotate(Vec3::DOWN);
            let b = upper.conjugate().rotate(Vec3::DOWN);
            if a.angle_to(b).to_degrees() > STILLNESS_LIMIT_DEG {
                self.clear();
                self.upper.push(upper);
                if let Some(f) = forearm {
                    self.forearm.push(f);
                }
                return AccumulateStatus::Moving;
            }
        }
        self.upper.push(upper);
        if let Some(f) = forearm {
            self.forearm.push(f);
        }
        if self.upper.len() >= SAMPLES_PER_POSE {
            AccumulateStatus::Done
        } else {
            AccumulateStatus::Collecting(self.progress())
        }
    }

    fn mean_axis(samples: &[Quat], world: Vec3) -> Option<Vec3> {
        samples
            .iter()
            .fold(Vec3::ZERO, |acc, q| acc + q.conjugate().rotate(world))
            .normalized()
    }

    pub fn has_forearm(&self) -> bool {
        self.forearm.len() * 2 >= self.upper.len() && !self.forearm.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RestCapture {
    pub upper_axis: Vec3,
    pub forearm_axis: Option<Vec3>,
    /// Upper sensor orientation at rest (used for the heading fallback).
    pub upper_rest: Quat,
}

/// Step 1: arm hanging at the side.
pub fn capture_rest(acc: &PoseAccumulator) -> Option<RestCapture> {
    if acc.upper.len() < SAMPLES_PER_POSE {
        return None;
    }
    Some(RestCapture {
        upper_axis: PoseAccumulator::mean_axis(&acc.upper, Vec3::DOWN)?,
        forearm_axis: if acc.has_forearm() {
            PoseAccumulator::mean_axis(&acc.forearm, Vec3::DOWN)
        } else {
            None
        },
        upper_rest: *acc.upper.last()?,
    })
}

#[derive(Debug, PartialEq)]
pub enum ForwardPoseError {
    NotRaisedEnough(f64),
    ElbowBent(f64),
}

impl std::fmt::Display for ForwardPoseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForwardPoseError::NotRaisedEnough(e) => write!(
                f,
                "Raise the arm further forward (now {e:.0} deg, need at least 25 deg)."
            ),
            ForwardPoseError::ElbowBent(d) => write!(
                f,
                "Keep the elbow straight: the two sensors differ by {d:.0} deg in tilt."
            ),
        }
    }
}

/// Step 2: arm raised forward with the elbow straight.
pub fn capture_forward(
    rest: &RestCapture,
    acc: &PoseAccumulator,
) -> Result<Calibration, ForwardPoseError> {
    let mut sum_u = (0.0, 0.0);
    let mut sum_f = (0.0, 0.0);
    let mut elev_u = 0.0;
    let mut elev_f = 0.0;
    let n = acc.upper.len().max(1) as f64;
    for q in &acc.upper {
        let d = q.rotate(rest.upper_axis);
        elev_u += d.angle_to(Vec3::DOWN).to_degrees();
        let h = d.y.atan2(d.x);
        sum_u.0 += h.cos();
        sum_u.1 += h.sin();
    }
    elev_u /= n;
    if elev_u < 25.0 {
        return Err(ForwardPoseError::NotRaisedEnough(elev_u));
    }
    let forward_heading = sum_u.1.atan2(sum_u.0);
    let mut forearm_yaw_offset = 0.0;
    if let (Some(axis), true) = (rest.forearm_axis, acc.has_forearm()) {
        let m = acc.forearm.len() as f64;
        for q in &acc.forearm {
            let d = q.rotate(axis);
            elev_f += d.angle_to(Vec3::DOWN).to_degrees();
            let h = d.y.atan2(d.x);
            sum_f.0 += h.cos();
            sum_f.1 += h.sin();
        }
        elev_f /= m;
        if (elev_u - elev_f).abs() > 20.0 {
            return Err(ForwardPoseError::ElbowBent((elev_u - elev_f).abs()));
        }
        forearm_yaw_offset = wrap_angle(forward_heading - sum_f.1.atan2(sum_f.0));
    }
    Ok(Calibration {
        upper_axis: rest.upper_axis,
        forearm_axis: rest.forearm_axis,
        forward_heading,
        forearm_yaw_offset,
        heading_measured: true,
    })
}

/// Rest-only calibration: elevation is exact, heading is estimated by assuming
/// the upper sensor's board normal (+Z) faces laterally (away from the body).
pub fn calibration_without_forward_pose(rest: &RestCapture, side: Side) -> Calibration {
    let lateral = rest.upper_rest.rotate(Vec3::Z);
    let lateral_heading = horizontal_heading(lateral, 0.2).unwrap_or(0.0);
    Calibration {
        upper_axis: rest.upper_axis,
        forearm_axis: rest.forearm_axis,
        forward_heading: wrap_angle(lateral_heading + side.sign() * FRAC_PI_2),
        forearm_yaw_offset: 0.0,
        heading_measured: false,
    }
}

/// Exponential smoothing for the 20 Hz hand position.
#[derive(Clone, Copy, Debug, Default)]
pub struct Smoother {
    value: Option<[f64; 2]>,
}

impl Smoother {
    pub fn update(&mut self, sample: [f64; 2], alpha: f64) -> [f64; 2] {
        let next = match self.value {
            Some(v) => [
                v[0] + alpha * (sample[0] - v[0]),
                v[1] + alpha * (sample[1] - v[1]),
            ],
            None => sample,
        };
        self.value = Some(next);
        next
    }

    pub fn reset(&mut self) {
        self.value = None;
    }
}

/// Synthesises sensor quaternions for a known arm pose (simulator and tests).
#[derive(Clone, Copy, Debug)]
pub struct SimArm {
    pub side: Side,
    pub elevation_deg: f64,
    pub plane_deg: f64,
    pub elbow_deg: f64,
    /// Heading the patient faces in the upper sensor's world frame.
    pub facing: f64,
    /// Yaw difference between the two sensors' world frames.
    pub forearm_world_yaw: f64,
    pub upper_mount: Quat,
    pub forearm_mount: Quat,
}

impl SimArm {
    pub fn new(side: Side) -> Self {
        Self {
            side,
            elevation_deg: 0.0,
            plane_deg: 0.0,
            elbow_deg: 0.0,
            facing: 0.7,
            forearm_world_yaw: -1.1,
            // Sensor +X along the segment, rotated arbitrarily about it.
            upper_mount: Quat::from_two_vectors(Vec3::X, Vec3::DOWN)
                * Quat::from_axis_angle(Vec3::X, 0.4),
            forearm_mount: Quat::from_two_vectors(Vec3::Y, Vec3::DOWN)
                * Quat::from_axis_angle(Vec3::Y, -0.9),
        }
    }

    /// Straight-elbow pose whose hand projects onto `(x, up)` (arm lengths).
    pub fn set_hand_target(&mut self, x: f64, up: f64) {
        let mut x = x;
        let mut z = up;
        let r = (x * x + z * z).sqrt();
        if r > 0.999 {
            x *= 0.999 / r;
            z *= 0.999 / r;
        }
        let y = (1.0 - x * x - z * z).max(0.0).sqrt();
        self.elevation_deg = (-z).clamp(-1.0, 1.0).acos().to_degrees();
        self.plane_deg = y.atan2(self.side.sign() * x).to_degrees();
        self.elbow_deg = 0.0;
    }

    fn body_directions(&self) -> (Quat, Quat) {
        let e = self.elevation_deg.to_radians();
        let p = self.plane_deg.to_radians();
        let u = Vec3::new(
            self.side.sign() * e.sin() * p.cos(),
            e.sin() * p.sin(),
            -e.cos(),
        );
        let shoulder = Quat::from_two_vectors(Vec3::DOWN, u);
        let elbow = Quat::from_axis_angle(Vec3::X, self.elbow_deg.to_radians());
        (shoulder, shoulder * elbow)
    }

    /// Quaternions as the firmware would report them: (upper, forearm).
    pub fn quats(&self) -> (Quat, Quat) {
        let (upper_body, forearm_body) = self.body_directions();
        let to_world = Quat::from_axis_angle(Vec3::Z, self.facing - FRAC_PI_2);
        let forearm_world = Quat::from_axis_angle(Vec3::Z, -self.forearm_world_yaw);
        (
            to_world * upper_body * self.upper_mount,
            forearm_world * to_world * forearm_body * self.forearm_mount,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calibrate(sim: &mut SimArm) -> Calibration {
        let mut acc = PoseAccumulator::default();
        sim.elevation_deg = 0.0;
        sim.elbow_deg = 0.0;
        for _ in 0..SAMPLES_PER_POSE {
            let (u, f) = sim.quats();
            acc.push(u, Some(f));
        }
        let rest = capture_rest(&acc).expect("rest capture");
        acc.clear();
        sim.elevation_deg = 60.0;
        sim.plane_deg = 90.0;
        for _ in 0..SAMPLES_PER_POSE {
            let (u, f) = sim.quats();
            acc.push(u, Some(f));
        }
        capture_forward(&rest, &acc).expect("forward capture")
    }

    #[test]
    fn recovers_abduction_plane_and_elbow_for_both_sides() {
        for side in [Side::Right, Side::Left] {
            let mut sim = SimArm::new(side);
            let cal = calibrate(&mut sim);
            for (e, p, el) in [
                (90.0, 0.0, 0.0),
                (45.0, 10.0, 30.0),
                (150.0, -20.0, 90.0),
                (60.0, 90.0, 0.0),
            ] {
                sim.elevation_deg = e;
                sim.plane_deg = p;
                sim.elbow_deg = el;
                let (u, f) = sim.quats();
                let pose = cal.pose(u, Some(f), side);
                assert!(
                    (pose.elevation_deg - e).abs() < 0.5,
                    "{side:?} elevation {} vs {e}",
                    pose.elevation_deg
                );
                assert!(
                    (pose.plane_deg.unwrap() - p).abs() < 0.5,
                    "{side:?} plane {:?} vs {p}",
                    pose.plane_deg
                );
                assert!(
                    (pose.elbow_flexion_deg.unwrap() - el).abs() < 0.5,
                    "{side:?} elbow"
                );
            }
        }
    }

    #[test]
    fn hand_projection_follows_target_and_mirrors_by_side() {
        for side in [Side::Right, Side::Left] {
            let mut sim = SimArm::new(side);
            let cal = calibrate(&mut sim);
            for target in [[0.4, -0.5], [-0.3, 0.2], [0.6, 0.6]] {
                let x = target[0] * side.sign();
                sim.set_hand_target(x, target[1]);
                let (u, f) = sim.quats();
                let pose = cal.pose(u, Some(f), side);
                assert!(
                    (pose.hand[0] - x).abs() < 0.01,
                    "hand x {} vs {x}",
                    pose.hand[0]
                );
                assert!((pose.hand[1] - target[1]).abs() < 0.01);
            }
        }
    }

    #[test]
    fn abduction_works_with_upper_sensor_only() {
        let mut sim = SimArm::new(Side::Right);
        let cal = calibrate(&mut sim);
        sim.elevation_deg = 75.0;
        sim.plane_deg = 0.0;
        let (u, _) = sim.quats();
        let pose = cal.pose(u, None, Side::Right);
        assert!((pose.elevation_deg - 75.0).abs() < 0.5);
        assert!(pose.elbow_flexion_deg.is_none());
    }

    #[test]
    fn forward_pose_rejects_low_arm_and_bent_elbow() {
        let mut sim = SimArm::new(Side::Right);
        let mut acc = PoseAccumulator::default();
        for _ in 0..SAMPLES_PER_POSE {
            let (u, f) = sim.quats();
            acc.push(u, Some(f));
        }
        let rest = capture_rest(&acc).unwrap();
        acc.clear();
        sim.elevation_deg = 10.0;
        sim.plane_deg = 90.0;
        for _ in 0..SAMPLES_PER_POSE {
            let (u, f) = sim.quats();
            acc.push(u, Some(f));
        }
        assert!(matches!(
            capture_forward(&rest, &acc),
            Err(ForwardPoseError::NotRaisedEnough(_))
        ));
        acc.clear();
        sim.elevation_deg = 50.0;
        sim.elbow_deg = 70.0;
        for _ in 0..SAMPLES_PER_POSE {
            let (u, f) = sim.quats();
            acc.push(u, Some(f));
        }
        assert!(matches!(
            capture_forward(&rest, &acc),
            Err(ForwardPoseError::ElbowBent(_))
        ));
    }

    #[test]
    fn accumulator_restarts_when_arm_moves() {
        let mut sim = SimArm::new(Side::Right);
        let mut acc = PoseAccumulator::default();
        let (u, f) = sim.quats();
        acc.push(u, Some(f));
        sim.elevation_deg = 20.0;
        let (u, f) = sim.quats();
        assert_eq!(acc.push(u, Some(f)), AccumulateStatus::Moving);
        assert!(acc.progress() < 0.1);
    }

    #[test]
    fn rest_only_calibration_still_measures_elevation() {
        let mut sim = SimArm::new(Side::Right);
        let mut acc = PoseAccumulator::default();
        for _ in 0..SAMPLES_PER_POSE {
            let (u, f) = sim.quats();
            acc.push(u, Some(f));
        }
        let cal = calibration_without_forward_pose(&capture_rest(&acc).unwrap(), Side::Right);
        assert!(!cal.heading_measured);
        sim.elevation_deg = 80.0;
        let (u, f) = sim.quats();
        assert!((cal.pose(u, Some(f), Side::Right).elevation_deg - 80.0).abs() < 0.5);
    }

    #[test]
    fn partial_samples_and_low_elevation_leave_fields_undefined() {
        let mut sim = SimArm::new(Side::Right);
        let mut acc = PoseAccumulator::default();
        for _ in 1..SAMPLES_PER_POSE {
            let (u, f) = sim.quats();
            acc.push(u, Some(f));
        }
        assert!(
            capture_rest(&acc).is_none(),
            "the rest pose needs a full still sample"
        );

        for (forearm_samples, has_axis) in [
            (SAMPLES_PER_POSE / 2 - 1, false),
            (SAMPLES_PER_POSE / 2, true),
        ] {
            acc.clear();
            for i in 0..SAMPLES_PER_POSE {
                let (u, f) = sim.quats();
                acc.push(u, (i < forearm_samples).then_some(f));
            }
            let rest = capture_rest(&acc).expect("upper-arm samples are enough");
            assert_eq!(
                rest.forearm_axis.is_some(),
                has_axis,
                "forearm axis needs the forearm in at least half the samples ({forearm_samples} of {SAMPLES_PER_POSE})"
            );
        }

        let cal = calibrate(&mut sim);
        for (elevation, defined) in [(10.0, false), (20.0, true)] {
            sim.elevation_deg = elevation;
            sim.plane_deg = 0.0;
            let (u, f) = sim.quats();
            let pose = cal.pose(u, Some(f), Side::Right);
            assert_eq!(
                pose.plane_deg.is_some(),
                defined,
                "plane of elevation at {elevation} deg: {:?}",
                pose.plane_deg
            );
        }
    }
}
