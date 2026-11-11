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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensor::{LinkState, Source};

    #[test]
    fn simulator_streams_through_the_hub_until_the_source_changes() {
        // Never driven: the Bluetooth task is queued but cannot run, so no radio is used.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime");
        let hub = SensorHub::new(runtime.handle(), eframe::egui::Context::default(), 0);
        let mut sim = SimDriver::new(Side::Right);
        sim.tick(&hub, 0.2);
        assert!(
            hub.drain().is_empty(),
            "packets are dropped until the simulator is the source"
        );

        hub.start_simulation();
        assert_eq!(
            (hub.source(), hub.state()),
            (Source::Simulator, LinkState::Simulated)
        );
        sim.tick(&hub, 0.12);
        let frames = hub.drain();
        assert_eq!(frames.len(), 2, "20 Hz: two packets in 120 ms");
        assert!(
            frames
                .iter()
                .all(|f| f.frame.upper.is_some() && f.frame.forearm.is_some()),
            "both segments are simulated"
        );
        sim.tick(&hub, 0.01);
        assert!(hub.drain().is_empty(), "partial intervals carry over");
        sim.tick(&hub, 0.04);
        assert_eq!(hub.drain().len(), 1);
        assert_eq!(hub.stats().packets, 3);

        sim.input = SimInput::HandTarget([3.0, 4.0]);
        sim.tick(&hub, 0.01);
        let far = sim.arm;
        sim.arm.set_hand_target(0.6, 0.8);
        assert!(
            (far.elevation_deg - sim.arm.elevation_deg).abs() < 1e-9,
            "targets out of reach clamp to the arm's length"
        );
        assert!(
            (140.0..=145.0).contains(&far.elevation_deg),
            "hand up and out: {}",
            far.elevation_deg
        );

        sim.forward_pose();
        assert_eq!(
            sim.input,
            SimInput::Sliders,
            "presets take over from the mouse"
        );
        sim.arm.elbow_deg = 35.0;
        sim.set_side(Side::Left);
        assert_eq!(sim.arm.side, Side::Left);
        assert_eq!(
            (sim.arm.elevation_deg, sim.arm.plane_deg, sim.arm.elbow_deg),
            (60.0, 90.0, 35.0),
            "switching arms keeps the slider pose"
        );
        sim.rest_pose();
        assert_eq!(
            (sim.arm.elevation_deg, sim.arm.plane_deg, sim.arm.elbow_deg),
            (0.0, 0.0, 0.0)
        );

        hub.push_simulated("1,2,3");
        let stats = hub.stats();
        assert_eq!(stats.invalid_packets, 1);
        assert_eq!(
            stats.last_invalid.as_deref(),
            Some("expected 8 values, got 3")
        );

        hub.disconnect();
        assert_eq!((hub.source(), hub.state()), (Source::None, LinkState::Idle));
        assert_eq!(
            hub.stats().packets,
            0,
            "stats restart when the source changes"
        );
        sim.tick(&hub, 0.2);
        assert!(
            hub.drain().is_empty(),
            "late simulator packets are ignored after disconnecting"
        );
        hub.start_simulation();
        sim.tick(&hub, 0.06);
        assert_eq!(hub.drain().len(), 1, "simulation resumes cleanly");
    }
}
