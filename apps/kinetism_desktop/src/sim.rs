//! Simulated `Physio_A` sensor for demos without hardware.
//!
//! Generates real quaternion packets from a virtual arm and feeds them through
//! the same parser and calibration pipeline as BLE data.

use crate::clinical::Side;
use crate::game::Pt;
use crate::kinematics::SimArm;
use crate::sensor::SensorHub;
use crate::sensor::frame::encode_frame;

const PACKET_INTERVAL: f64 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SimInput {
    /// Elevation / plane / elbow sliders.
    Sliders,
    /// Hand follows a target point (mouse pointer or autopilot).
    HandTarget(Pt),
}

pub struct SimDriver {
    pub arm: SimArm,
    pub input: SimInput,
    accum: f64,
}

impl SimDriver {
    pub fn new(side: Side) -> Self {
        Self {
            arm: SimArm::new(side),
            input: SimInput::Sliders,
            accum: 0.0,
        }
    }

    pub fn set_side(&mut self, side: Side) {
        if self.arm.side != side {
            let mut arm = SimArm::new(side);
            arm.elevation_deg = self.arm.elevation_deg;
            arm.plane_deg = self.arm.plane_deg;
            arm.elbow_deg = self.arm.elbow_deg;
            self.arm = arm;
        }
    }

    pub fn rest_pose(&mut self) {
        self.input = SimInput::Sliders;
        self.arm.elevation_deg = 0.0;
        self.arm.plane_deg = 0.0;
        self.arm.elbow_deg = 0.0;
    }

    pub fn forward_pose(&mut self) {
        self.input = SimInput::Sliders;
        self.arm.elevation_deg = 60.0;
        self.arm.plane_deg = 90.0;
        self.arm.elbow_deg = 0.0;
    }

    /// Emit packets at the firmware rate (20 Hz).
    pub fn tick(&mut self, hub: &SensorHub, dt: f64) {
        if let SimInput::HandTarget(p) = self.input {
            self.arm.set_hand_target(p[0], p[1]);
        }
        self.accum += dt;
        while self.accum >= PACKET_INTERVAL {
            self.accum -= PACKET_INTERVAL;
            let (u, f) = self.arm.quats();
            hub.push_simulated(&encode_frame(u, Some(f)));
        }
    }
}
