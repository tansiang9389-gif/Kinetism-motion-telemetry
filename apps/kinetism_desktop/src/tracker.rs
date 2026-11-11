//! Live arm tracking: calibration flow, smoothed pose, peaks and history.

use crate::clinical::Side;
use crate::game::Pt;
use crate::kinematics::{
    AccumulateStatus, ArmPose, Calibration, PoseAccumulator, RestCapture, Smoother,
    calibration_without_forward_pose, capture_forward, capture_rest,
};
use crate::sensor::TimedFrame;
use std::collections::VecDeque;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalStep {
    /// Waiting for the user to start capturing the rest pose.
    Rest,
    CapturingRest,
    /// Waiting for the user to start capturing the forward pose.
    Forward,
    CapturingForward,
    Done,
}

pub const HISTORY_SECONDS: f64 = 15.0;
/// Upper-arm data older than this is stale: recording and the game pause.
pub const LIVE_DATA_SECONDS: f32 = 1.5;

pub struct ArmTracker {
    pub side: Side,
    pub step: CalStep,
    pub calibration: Option<Calibration>,
    pub message: Option<String>,
    pub moving_warning: bool,
    rest: Option<RestCapture>,
    acc: PoseAccumulator,
    pub pose: Option<ArmPose>,
    pub hand: Option<Pt>,
    smoother: Smoother,
    pub peak_elevation: f64,
    pub peak_elbow: f64,
    /// Pose in which `peak_elevation` / `peak_elbow` was reached. An attempt is
    /// graded on its peak, so posture checks must use the peak's posture too,
    /// not wherever the arm is when Record is pressed.
    pub peak_pose: Option<ArmPose>,
    pub peak_elbow_pose: Option<ArmPose>,
    /// (seconds since start, shoulder elevation, elbow flexion)
    pub history: VecDeque<(f64, f64, Option<f64>)>,
    started: Instant,
    /// Arrival of the latest frame carrying the upper-arm orientation.
    pub last_frame_at: Option<Instant>,
}

impl ArmTracker {
    pub fn new(side: Side) -> Self {
        Self {
            side,
            step: CalStep::Rest,
            calibration: None,
            message: None,
            moving_warning: false,
            rest: None,
            acc: PoseAccumulator::default(),
            pose: None,
            hand: None,
            smoother: Smoother::default(),
            peak_elevation: 0.0,
            peak_elbow: 0.0,
            peak_pose: None,
            peak_elbow_pose: None,
            history: VecDeque::new(),
            started: Instant::now(),
            last_frame_at: None,
        }
    }

    pub fn is_calibrated(&self) -> bool {
        self.calibration.is_some() && self.step == CalStep::Done
    }

    pub fn progress(&self) -> f32 {
        self.acc.progress()
    }

    /// True while upper-arm orientation keeps arriving. Frames carrying only the
    /// forearm sensor cannot move the shoulder angle, so they do not count.
    pub fn has_live_data(&self) -> bool {
        self.last_frame_at
            .is_some_and(|t| t.elapsed().as_secs_f32() <= LIVE_DATA_SECONDS)
    }

    pub fn recalibrate(&mut self) {
        self.step = CalStep::Rest;
        self.calibration = None;
        self.rest = None;
        self.acc.clear();
        self.message = None;
        self.moving_warning = false;
        self.pose = None;
        self.hand = None;
        self.smoother.reset();
        // Angles from the old calibration mean nothing under the new one.
        self.history.clear();
        self.reset_peaks();
    }

    pub fn set_side(&mut self, side: Side) {
        if self.side != side {
            self.side = side;
            // Side changes the plane sign and the heading fallback.
            self.recalibrate();
        }
    }

    pub fn start_rest_capture(&mut self) {
        self.acc.clear();
        self.message = None;
        self.moving_warning = false;
        self.step = CalStep::CapturingRest;
    }

    pub fn start_forward_capture(&mut self) {
        self.acc.clear();
        self.message = None;
        self.moving_warning = false;
        self.step = CalStep::CapturingForward;
    }

    pub fn skip_forward_pose(&mut self) {
        if let Some(rest) = &self.rest {
            self.calibration = Some(calibration_without_forward_pose(rest, self.side));
            self.step = CalStep::Done;
            self.message = Some(
                "Rest-only calibration: elevation is accurate; plane check and the game use an estimated heading."
                    .into(),
            );
            self.reset_peaks();
        }
    }

    pub fn reset_peaks(&mut self) {
        self.peak_elevation = self.pose.map_or(0.0, |p| p.elevation_deg);
        self.peak_elbow = self.pose.and_then(|p| p.elbow_flexion_deg).unwrap_or(0.0);
        self.peak_pose = self.pose;
        self.peak_elbow_pose = self.pose.filter(|p| p.elbow_flexion_deg.is_some());
    }

    fn track_peaks(&mut self, pose: ArmPose) {
        if pose.elevation_deg > self.peak_elevation || self.peak_pose.is_none() {
            self.peak_pose = Some(pose);
        }
        self.peak_elevation = self.peak_elevation.max(pose.elevation_deg);
        if let Some(e) = pose.elbow_flexion_deg {
            if e > self.peak_elbow || self.peak_elbow_pose.is_none() {
                self.peak_elbow_pose = Some(pose);
            }
            self.peak_elbow = self.peak_elbow.max(e);
        }
    }

    pub fn feed(&mut self, frames: &[TimedFrame]) {
        for tf in frames {
            let Some(upper) = tf.frame.upper else {
                continue;
            };
            self.last_frame_at = Some(tf.at);
            match self.step {
                CalStep::CapturingRest | CalStep::CapturingForward => {
                    match self.acc.push(upper, tf.frame.forearm) {
                        AccumulateStatus::Moving => self.moving_warning = true,
                        AccumulateStatus::Collecting(_) => {}
                        AccumulateStatus::Done => {
                            self.moving_warning = false;
                            self.finish_capture();
                        }
                    }
                }
                _ => {}
            }
            if let Some(cal) = self.calibration {
                let pose = cal.pose(upper, tf.frame.forearm, self.side);
                let hand = self.smoother.update(pose.hand, 0.45);
                self.hand = Some(hand);
                self.track_peaks(pose);
                let t = tf.at.duration_since(self.started).as_secs_f64();
                self.history
                    .push_back((t, pose.elevation_deg, pose.elbow_flexion_deg));
                while self
                    .history
                    .front()
                    .is_some_and(|(t0, _, _)| t - t0 > HISTORY_SECONDS)
                {
                    self.history.pop_front();
                }
                self.pose = Some(pose);
            }
        }
    }

    fn finish_capture(&mut self) {
        match self.step {
            CalStep::CapturingRest => {
                self.rest = capture_rest(&self.acc);
                self.step = if self.rest.is_some() {
                    CalStep::Forward
                } else {
                    self.message = Some("Could not read the sensors; try again.".into());
                    CalStep::Rest
                };
            }
            CalStep::CapturingForward => {
                let Some(rest) = self.rest else {
                    self.step = CalStep::Rest;
                    return;
                };
                match capture_forward(&rest, &self.acc) {
                    Ok(cal) => {
                        self.calibration = Some(cal);
                        self.step = CalStep::Done;
                        self.message = None;
                        self.smoother.reset();
                        self.history.clear();
                        self.reset_peaks();
                    }
                    Err(e) => {
                        self.message = Some(e.to_string());
                        self.step = CalStep::Forward;
                    }
                }
            }
            _ => {}
        }
        self.acc.clear();
    }

    pub fn now_seconds(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    pub fn forearm_missing(&self) -> bool {
        self.calibration.is_some_and(|c| c.forearm_axis.is_none())
            || self.pose.is_some_and(|p| p.forearm_dir.is_none())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinematics::{SAMPLES_PER_POSE, SimArm};
    use crate::sensor::frame::SensorFrame;
    use std::time::Duration;

    fn frame(sim: &SimArm, forearm: bool) -> SensorFrame {
        let (upper, fore) = sim.quats();
        SensorFrame {
            upper: Some(upper),
            forearm: forearm.then_some(fore),
        }
    }

    fn frames(sim: &SimArm, n: usize, forearm: bool) -> Vec<TimedFrame> {
        (0..n)
            .map(|_| TimedFrame {
                frame: frame(sim, forearm),
                at: Instant::now(),
            })
            .collect()
    }

    /// Rest pose then forward pose, driven the way the Measure screen buttons do.
    fn calibrate(tracker: &mut ArmTracker, sim: &mut SimArm, forearm: bool) {
        sim.elevation_deg = 0.0;
        sim.plane_deg = 0.0;
        sim.elbow_deg = 0.0;
        tracker.start_rest_capture();
        tracker.feed(&frames(sim, SAMPLES_PER_POSE, forearm));
        assert_eq!(tracker.step, CalStep::Forward, "rest pose captured");
        sim.elevation_deg = 60.0;
        sim.plane_deg = 90.0;
        tracker.start_forward_capture();
        tracker.feed(&frames(sim, SAMPLES_PER_POSE, forearm));
        assert!(
            tracker.is_calibrated(),
            "forward pose completes calibration: {:?}",
            tracker.message
        );
    }

    #[test]
    fn calibration_restarts_on_movement_then_tracks_pose_peaks_and_history() {
        let mut sim = SimArm::new(Side::Right);
        let mut tracker = ArmTracker::new(Side::Right);
        assert!(!tracker.is_calibrated(), "a new tracker needs calibration");
        tracker.start_rest_capture();
        assert_eq!(tracker.step, CalStep::CapturingRest);
        tracker.feed(&frames(&sim, 5, true));
        sim.elevation_deg = 25.0;
        tracker.feed(&frames(&sim, 1, true));
        assert!(tracker.moving_warning, "moving during capture warns");
        assert!(tracker.progress() < 0.1, "capture restarts after movement");
        assert_eq!(tracker.step, CalStep::CapturingRest, "still capturing");

        sim.elevation_deg = 0.0;
        tracker.feed(&frames(&sim, SAMPLES_PER_POSE, true));
        assert_eq!(
            tracker.step,
            CalStep::Forward,
            "a still rest pose is captured"
        );
        assert!(
            !tracker.moving_warning,
            "warning clears once a pose is captured"
        );

        sim.elevation_deg = 60.0;
        sim.plane_deg = 90.0;
        tracker.start_forward_capture();
        tracker.feed(&frames(&sim, SAMPLES_PER_POSE, true));
        assert!(
            tracker.is_calibrated(),
            "forward pose completes calibration"
        );
        assert!(
            tracker.message.is_none(),
            "no hint after a good calibration"
        );

        // The first pose after calibration seeds the smoother; later poses are smoothed.
        let before = tracker.hand.expect("hand position after calibration");
        sim.elevation_deg = 90.0;
        sim.plane_deg = 0.0;
        tracker.feed(&frames(&sim, 1, true));
        let pose = tracker.pose.expect("live pose after calibration");
        assert!(
            (pose.elevation_deg - 90.0).abs() < 0.5,
            "abduction {}",
            pose.elevation_deg
        );
        let hand = tracker.hand.expect("smoothed hand");
        for ((h, b), p) in hand.iter().zip(before).zip(pose.hand) {
            assert!(
                (h - (b + 0.45 * (p - b))).abs() < 1e-9,
                "hand moves 45% of the way to the new pose"
            );
        }

        sim.elevation_deg = 30.0;
        sim.elbow_deg = 40.0;
        tracker.feed(&frames(&sim, 1, true));
        assert!(
            (tracker.peak_elevation - 90.0).abs() < 0.5,
            "peak keeps the highest elevation, got {}",
            tracker.peak_elevation
        );
        assert!(
            (tracker.peak_elbow - 40.0).abs() < 0.5,
            "elbow peak {}",
            tracker.peak_elbow
        );
        tracker.reset_peaks();
        assert!(
            (tracker.peak_elevation - 30.0).abs() < 0.5,
            "reset starts from the current pose, got {}",
            tracker.peak_elevation
        );

        let base = Instant::now();
        for s in 0..=20u64 {
            tracker.feed(&[TimedFrame {
                frame: frame(&sim, true),
                at: base + Duration::from_secs(s),
            }]);
        }
        let first = tracker.history.front().expect("history").0;
        let last = tracker.history.back().expect("history").0;
        assert!(
            last - first <= HISTORY_SECONDS + 1e-6,
            "history spans at most 15 s, got {}",
            last - first
        );
        assert!(
            last - first >= HISTORY_SECONDS - 1.0,
            "history keeps the recent 15 s, got {}",
            last - first
        );

        tracker.recalibrate();
        assert_eq!(
            tracker.step,
            CalStep::Rest,
            "recalibrate restarts at step 1"
        );
        assert!(!tracker.is_calibrated());
        assert!(
            tracker.pose.is_none() && tracker.hand.is_none(),
            "no stale pose after recalibrating"
        );
    }

    #[test]
    fn forward_pose_errors_keep_step_two_and_explain_the_fix() {
        let mut sim = SimArm::new(Side::Left);
        let mut tracker = ArmTracker::new(Side::Left);
        tracker.start_forward_capture();
        tracker.feed(&frames(&sim, SAMPLES_PER_POSE, true));
        assert_eq!(
            tracker.step,
            CalStep::Rest,
            "a forward pose without a rest pose goes back to step 1"
        );
        assert!(tracker.calibration.is_none());

        tracker.start_rest_capture();
        tracker.feed(&frames(&sim, SAMPLES_PER_POSE, true));
        assert_eq!(tracker.step, CalStep::Forward);

        sim.elevation_deg = 10.0;
        sim.plane_deg = 90.0;
        tracker.start_forward_capture();
        tracker.feed(&frames(&sim, SAMPLES_PER_POSE, true));
        assert_eq!(tracker.step, CalStep::Forward, "a low arm stays on step 2");
        let hint = tracker.message.clone().unwrap_or_default();
        assert!(
            hint.starts_with("Raise the arm further forward") && hint.contains("at least 25 deg"),
            "low-arm hint: {hint}"
        );
        assert!(!tracker.is_calibrated());

        sim.elevation_deg = 50.0;
        sim.elbow_deg = 70.0;
        tracker.start_forward_capture();
        assert!(
            tracker.message.is_none(),
            "a new capture clears the old hint"
        );
        tracker.feed(&frames(&sim, SAMPLES_PER_POSE, true));
        let hint = tracker.message.clone().unwrap_or_default();
        assert!(
            hint.starts_with("Keep the elbow straight"),
            "bent-elbow hint: {hint}"
        );
        assert_eq!(tracker.step, CalStep::Forward);

        sim.elbow_deg = 0.0;
        tracker.start_forward_capture();
        tracker.feed(&frames(&sim, SAMPLES_PER_POSE, true));
        assert!(
            tracker.is_calibrated(),
            "retrying with a straight elbow succeeds: {:?}",
            tracker.message
        );
    }

    #[test]
    fn rest_only_calibration_needs_a_rest_pose_and_side_changes_recalibrate() {
        let mut sim = SimArm::new(Side::Right);
        let mut tracker = ArmTracker::new(Side::Right);
        tracker.skip_forward_pose();
        assert_eq!(
            tracker.step,
            CalStep::Rest,
            "skip before step 1 does nothing"
        );
        assert!(tracker.calibration.is_none());

        tracker.start_rest_capture();
        tracker.feed(&frames(&sim, SAMPLES_PER_POSE, true));
        tracker.skip_forward_pose();
        assert!(
            tracker.is_calibrated(),
            "skip after the rest pose calibrates"
        );
        assert!(
            !tracker.calibration.expect("calibration").heading_measured,
            "the heading is only estimated"
        );
        assert!(
            tracker
                .message
                .as_deref()
                .is_some_and(|m| m.starts_with("Rest-only calibration")),
            "rest-only hint: {:?}",
            tracker.message
        );
        sim.elevation_deg = 80.0;
        tracker.feed(&frames(&sim, 1, true));
        let elevation = tracker.pose.expect("pose").elevation_deg;
        assert!(
            (elevation - 80.0).abs() < 0.5,
            "elevation stays exact: {elevation}"
        );

        tracker.set_side(Side::Right);
        assert!(tracker.is_calibrated(), "same side keeps the calibration");
        tracker.set_side(Side::Left);
        assert_eq!(tracker.side, Side::Left);
        assert_eq!(
            tracker.step,
            CalStep::Rest,
            "the other arm needs a new calibration"
        );
        assert!(!tracker.is_calibrated() && tracker.pose.is_none());
        assert!(tracker.message.is_none(), "recalibration clears the hint");
    }

    #[test]
    fn upper_arm_only_stream_calibrates_without_an_elbow_angle() {
        let mut sim = SimArm::new(Side::Right);
        let mut tracker = ArmTracker::new(Side::Right);
        tracker.start_rest_capture();
        let (_, fore) = sim.quats();
        let forearm_only: Vec<TimedFrame> = (0..SAMPLES_PER_POSE)
            .map(|_| TimedFrame {
                frame: SensorFrame {
                    upper: None,
                    forearm: Some(fore),
                },
                at: Instant::now(),
            })
            .collect();
        tracker.feed(&forearm_only);
        assert_eq!(
            tracker.step,
            CalStep::CapturingRest,
            "without the upper-arm sensor calibration cannot progress"
        );
        assert_eq!(tracker.progress(), 0.0);

        // Wi-Fi KIN1 sensor: a single IMU on the upper arm.
        calibrate(&mut tracker, &mut sim, false);
        let cal = tracker.calibration.expect("calibration");
        assert!(
            cal.forearm_axis.is_none(),
            "no forearm axis without a forearm sensor"
        );
        assert_eq!(cal.forearm_yaw_offset, 0.0);
        assert!(
            tracker.forearm_missing(),
            "elbow flexion is flagged as unavailable"
        );

        sim.elevation_deg = 75.0;
        sim.plane_deg = 0.0;
        sim.elbow_deg = 30.0;
        tracker.feed(&frames(&sim, 1, true));
        let pose = tracker.pose.expect("pose");
        assert!(
            (pose.elevation_deg - 75.0).abs() < 0.5,
            "shoulder abduction works with one IMU: {}",
            pose.elevation_deg
        );
        assert!(
            pose.plane_deg.is_some_and(|p| p.abs() < 0.5),
            "measured heading: {:?}",
            pose.plane_deg
        );
        assert!(
            pose.elbow_flexion_deg.is_none(),
            "an uncalibrated forearm sensor is ignored"
        );

        // Two-sensor calibration, then the forearm sensor drops out.
        let mut both = ArmTracker::new(Side::Right);
        calibrate(&mut both, &mut sim, true);
        assert!(!both.forearm_missing(), "both segments are streaming");
        both.feed(&frames(&sim, 1, false));
        assert!(
            both.forearm_missing(),
            "a forearm sensor that stops is flagged"
        );
        assert!(both.pose.expect("pose").elbow_flexion_deg.is_none());
    }

    #[test]
    fn a_new_capture_clears_a_stale_movement_warning() {
        let mut sim = SimArm::new(Side::Right);
        let mut tracker = ArmTracker::new(Side::Right);
        let mut interrupted_rest = |tracker: &mut ArmTracker| {
            sim.elevation_deg = 0.0;
            tracker.start_rest_capture();
            tracker.feed(&frames(&sim, 3, true));
            sim.elevation_deg = 30.0;
            tracker.feed(&frames(&sim, 1, true));
            assert!(tracker.moving_warning, "moving during capture warns");
        };

        // Switching the sensor source or arm side mid-capture recalibrates.
        interrupted_rest(&mut tracker);
        tracker.recalibrate();
        assert!(!tracker.moving_warning, "recalibrate clears the warning");

        interrupted_rest(&mut tracker);
        tracker.start_rest_capture();
        assert!(!tracker.moving_warning, "a new rest capture starts clean");

        interrupted_rest(&mut tracker);
        sim.elevation_deg = 0.0;
        tracker.feed(&frames(&sim, SAMPLES_PER_POSE, true));
        assert_eq!(tracker.step, CalStep::Forward, "rest pose captured");
        sim.elevation_deg = 60.0;
        sim.plane_deg = 90.0;
        tracker.start_forward_capture();
        tracker.feed(&frames(&sim, 2, true));
        sim.elevation_deg = 20.0;
        tracker.feed(&frames(&sim, 1, true));
        assert!(
            tracker.moving_warning,
            "moving during the forward capture warns"
        );
        tracker.start_forward_capture();
        assert!(
            !tracker.moving_warning,
            "a new forward capture starts clean"
        );
    }

    #[test]
    fn peaks_keep_the_posture_they_were_reached_in_until_reset() {
        let mut sim = SimArm::new(Side::Right);
        let mut tracker = ArmTracker::new(Side::Right);
        calibrate(&mut tracker, &mut sim, true);
        // Swing forward to 95 deg with a bent elbow, curl the elbow lower down,
        // then rest the arm near the side.
        for (elevation, plane, elbow) in
            [(95.0, 80.0, 40.0), (30.0, 0.0, 100.0), (20.0, 10.0, 10.0)]
        {
            sim.elevation_deg = elevation;
            sim.plane_deg = plane;
            sim.elbow_deg = elbow;
            tracker.feed(&frames(&sim, 1, true));
        }
        let near =
            |value: Option<f64>, expected: f64| value.is_some_and(|v| (v - expected).abs() < 0.5);
        let peak = tracker.peak_pose.expect("pose at the shoulder peak");
        assert!((tracker.peak_elevation - 95.0).abs() < 0.5);
        assert_eq!(peak.elevation_deg, tracker.peak_elevation);
        assert!(
            near(peak.plane_deg, 80.0) && near(peak.elbow_flexion_deg, 40.0),
            "the peak's own plane and elbow: {peak:?}"
        );
        let elbow_peak = tracker.peak_elbow_pose.expect("pose at the elbow peak");
        assert!((tracker.peak_elbow - 100.0).abs() < 0.5);
        assert!(
            (elbow_peak.elevation_deg - 30.0).abs() < 0.5 && near(elbow_peak.plane_deg, 0.0),
            "{elbow_peak:?}"
        );
        let now = tracker.pose.expect("current pose");
        assert!(near(now.plane_deg, 10.0), "the arm is back near the side");

        tracker.reset_peaks();
        assert_eq!(
            tracker.peak_pose,
            Some(now),
            "a reset starts from the current pose"
        );
        assert_eq!(tracker.peak_elbow_pose, Some(now));

        tracker.recalibrate();
        assert_eq!((tracker.peak_elevation, tracker.peak_elbow), (0.0, 0.0));
        assert!(tracker.peak_pose.is_none() && tracker.peak_elbow_pose.is_none());
        assert!(
            tracker.history.is_empty(),
            "no angle trace from the old calibration"
        );
    }

    #[test]
    fn forearm_only_frames_do_not_make_a_frozen_angle_look_live() {
        let mut sim = SimArm::new(Side::Right);
        let mut tracker = ArmTracker::new(Side::Right);
        assert!(!tracker.has_live_data(), "no data before the first frame");
        calibrate(&mut tracker, &mut sim, true);
        assert!(tracker.has_live_data(), "upper-arm frames are live data");

        // The upper-arm IMU stopped 5 s ago; the forearm sensor keeps streaming.
        let stopped = Instant::now()
            .checked_sub(Duration::from_secs(5))
            .expect("an instant 5 s ago");
        tracker.feed(&[TimedFrame {
            frame: frame(&sim, true),
            at: stopped,
        }]);
        assert!(
            !tracker.has_live_data(),
            "upper-arm data from 5 s ago is stale"
        );
        let frozen = tracker.pose.expect("pose").elevation_deg;
        sim.elevation_deg = 90.0;
        let (_, fore) = sim.quats();
        let forearm_only: Vec<TimedFrame> = (0..10)
            .map(|_| TimedFrame {
                frame: SensorFrame {
                    upper: None,
                    forearm: Some(fore),
                },
                at: Instant::now(),
            })
            .collect();
        tracker.feed(&forearm_only);
        assert!(
            !tracker.has_live_data(),
            "forearm-only frames keep the data stale, so Record and the game pause"
        );
        assert_eq!(tracker.last_frame_at, Some(stopped));
        assert_eq!(
            tracker.pose.expect("pose").elevation_deg,
            frozen,
            "the shoulder angle cannot change without the upper arm"
        );

        tracker.feed(&frames(&sim, 1, true));
        assert!(tracker.has_live_data(), "upper-arm data resumes");
    }
}
