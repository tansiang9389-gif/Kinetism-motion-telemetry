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
    /// (seconds since start, shoulder elevation, elbow flexion)
    pub history: VecDeque<(f64, f64, Option<f64>)>,
    started: Instant,
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

    pub fn recalibrate(&mut self) {
        self.step = CalStep::Rest;
        self.calibration = None;
        self.rest = None;
        self.acc.clear();
        self.message = None;
        self.pose = None;
        self.hand = None;
        self.smoother.reset();
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
        self.step = CalStep::CapturingRest;
    }

    pub fn start_forward_capture(&mut self) {
        self.acc.clear();
        self.message = None;
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
    }

    pub fn feed(&mut self, frames: &[TimedFrame]) {
        for tf in frames {
            self.last_frame_at = Some(tf.at);
            let Some(upper) = tf.frame.upper else {
                continue;
            };
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
                self.peak_elevation = self.peak_elevation.max(pose.elevation_deg);
                if let Some(e) = pose.elbow_flexion_deg {
                    self.peak_elbow = self.peak_elbow.max(e);
                }
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
