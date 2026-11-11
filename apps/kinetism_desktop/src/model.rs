//! Patient, measurement and game records shared with the phone app via Firestore.

use crate::clinical::{Anthropometrics, Movement, Muscle, Side};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const SOURCE_DESKTOP: &str = "kinetism-desktop";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Patient {
    pub id: String,
    pub name: String,
    pub code: String,
    pub arm: Option<Anthropometrics>,
    pub affected_side: Option<Side>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MmtSource {
    /// Suggested by the automatic grading.
    Auto,
    /// Confirmed or overridden by a physiotherapist.
    Physio,
}

impl MmtSource {
    pub fn key(self) -> &'static str {
        match self {
            MmtSource::Auto => "auto",
            MmtSource::Physio => "physio",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MeasurementRecord {
    pub id: String,
    pub patient_id: String,
    pub patient_name: String,
    pub body_part: String,
    pub movement: Movement,
    pub side: Option<Side>,
    pub arm: Anthropometrics,
    pub angle_degrees: f64,
    pub peak_angle_degrees: Option<f64>,
    pub force_newtons: f64,
    pub torque_nm: Option<f64>,
    pub mmt_grade: u8,
    pub mmt_title: String,
    pub mmt_rationale: String,
    pub mmt_source: MmtSource,
    pub auto_mmt_grade: Option<u8>,
    pub attempt_number: u32,
    pub elbow_flexion_degrees: Option<f64>,
    pub plane_degrees: Option<f64>,
    pub recorded_at: Option<DateTime<Utc>>,
    pub source: String,
}

impl MeasurementRecord {
    pub fn muscles(&self) -> &'static [Muscle] {
        self.movement.muscles()
    }

    pub fn badge(&self) -> String {
        format!("Grade {} / 5 ({})", self.mmt_grade, self.mmt_title)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GameLevel {
    AppleTrace,
    PnfD1,
    PnfD2,
}

impl GameLevel {
    pub const ALL: [GameLevel; 3] = [GameLevel::AppleTrace, GameLevel::PnfD1, GameLevel::PnfD2];

    pub fn index(self) -> u8 {
        match self {
            GameLevel::AppleTrace => 0,
            GameLevel::PnfD1 => 1,
            GameLevel::PnfD2 => 2,
        }
    }

    pub fn from_index(i: i64) -> Option<GameLevel> {
        GameLevel::ALL
            .into_iter()
            .find(|l| i64::from(l.index()) == i)
    }

    pub fn key(self) -> &'static str {
        match self {
            GameLevel::AppleTrace => "apple_trace",
            GameLevel::PnfD1 => "pnf_d1",
            GameLevel::PnfD2 => "pnf_d2",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            GameLevel::AppleTrace => "Level 0 - Draw the Apple",
            GameLevel::PnfD1 => "Level 1 - PNF D1 Slice",
            GameLevel::PnfD2 => "Level 2 - PNF D2 Slice",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            GameLevel::AppleTrace => "L0 Apple",
            GameLevel::PnfD1 => "L1 D1",
            GameLevel::PnfD2 => "L2 D2",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            GameLevel::AppleTrace => {
                "Trace the apple outline in the air with your hand. Finish the outline to slice the apple!"
            }
            GameLevel::PnfD1 => {
                "D1 flexion: swipe from the same-side hip up across your face to the opposite ear, slicing the fruit. Return to the basket at the hip."
            }
            GameLevel::PnfD2 => {
                "D2 flexion: swipe from the opposite hip up and out above your shoulder (like drawing a sword), slicing the fruit. Return to the basket."
            }
        }
    }

    pub fn muscles(self) -> &'static [Muscle] {
        match self {
            GameLevel::AppleTrace => &[
                Muscle::MiddleDeltoid,
                Muscle::AnteriorDeltoid,
                Muscle::Supraspinatus,
            ],
            GameLevel::PnfD1 => &[Muscle::AnteriorDeltoid, Muscle::PectoralisMajor],
            GameLevel::PnfD2 => &[
                Muscle::MiddleDeltoid,
                Muscle::Supraspinatus,
                Muscle::Infraspinatus,
            ],
        }
    }

    pub fn next(self) -> Option<GameLevel> {
        match self {
            GameLevel::AppleTrace => Some(GameLevel::PnfD1),
            GameLevel::PnfD1 => Some(GameLevel::PnfD2),
            GameLevel::PnfD2 => None,
        }
    }

    pub fn previous(self) -> Option<GameLevel> {
        match self {
            GameLevel::AppleTrace => None,
            GameLevel::PnfD1 => Some(GameLevel::AppleTrace),
            GameLevel::PnfD2 => Some(GameLevel::PnfD1),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GameSession {
    pub id: String,
    pub patient_id: String,
    pub patient_name: String,
    pub side: Side,
    pub level: GameLevel,
    pub score_percent: f64,
    pub points: i64,
    pub stars: u8,
    pub reps: u32,
    pub clean_reps: u32,
    pub bombs_hit: u32,
    pub max_combo: u32,
    pub accuracy_percent: f64,
    pub completion_percent: f64,
    pub duration_seconds: f64,
    pub max_elevation_deg: f64,
    pub recorded_at: Option<DateTime<Utc>>,
    pub source: String,
}

/// Score needed on a level to unlock the next one.
pub const UNLOCK_SCORE_PERCENT: f64 = 80.0;

pub fn new_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{:04x}",
        Utc::now().timestamp_micros(),
        fastrand::u16(..)
    )
}
