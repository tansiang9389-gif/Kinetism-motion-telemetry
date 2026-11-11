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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clinical::{grade_shoulder_abduction, mmt_title};
    use std::collections::HashSet;

    fn record(grade: u8, title: &str) -> MeasurementRecord {
        MeasurementRecord {
            id: new_id("rec"),
            patient_id: "p1".into(),
            patient_name: "Siti".into(),
            body_part: Movement::ShoulderAbduction.body_part().into(),
            movement: Movement::ShoulderAbduction,
            side: Some(Side::Left),
            arm: Anthropometrics::default(),
            angle_degrees: 40.0,
            peak_angle_degrees: Some(42.0),
            force_newtons: 20.0,
            torque_nm: None,
            mmt_grade: grade,
            mmt_title: title.into(),
            mmt_rationale: String::new(),
            mmt_source: MmtSource::Auto,
            auto_mmt_grade: Some(grade),
            attempt_number: 1,
            elbow_flexion_degrees: None,
            plane_degrees: None,
            recorded_at: None,
            source: SOURCE_DESKTOP.into(),
        }
    }

    #[test]
    fn new_id_is_prefix_then_creation_time_then_four_hex_digits() {
        let before = Utc::now().timestamp_micros();
        let id = new_id("game-session");
        let after = Utc::now().timestamp_micros();
        let mut parts = id.rsplitn(3, '-');
        let (suffix, micros, prefix) = (
            parts.next().unwrap(),
            parts.next().unwrap(),
            parts.next().unwrap(),
        );
        assert_eq!(prefix, "game-session", "{id}");
        let micros: i64 = micros
            .parse()
            .unwrap_or_else(|_| panic!("no timestamp in {id}"));
        assert!(
            (before..=after).contains(&micros),
            "{id} was made between {before} and {after}"
        );
        assert!(
            suffix.len() == 4
                && suffix
                    .chars()
                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "{id}"
        );
    }

    #[test]
    fn ids_made_one_after_another_never_collide() {
        let mut ids = HashSet::new();
        let mut suffixes = HashSet::new();
        for _ in 0..200 {
            let id = new_id("rec");
            suffixes.insert(id.rsplit('-').next().unwrap().to_string());
            ids.insert(id);
            // Saves are user actions: never two in the same microsecond.
            let made = Utc::now().timestamp_micros();
            while Utc::now().timestamp_micros() == made {}
        }
        assert_eq!(ids.len(), 200, "every id is unique");
        assert!(
            suffixes.len() >= 190,
            "the suffix is random, not a counter or constant: {} distinct",
            suffixes.len()
        );
    }

    #[test]
    fn badge_shows_the_saved_grade_including_a_physio_override() {
        for peak in [0.0, 10.0, 45.0, 95.0] {
            let mmt = grade_shoulder_abduction(peak, None, None);
            assert_eq!(
                record(mmt.grade, &mmt.title).badge(),
                mmt.badge(),
                "the saved record repeats the live grade chip at {peak} deg"
            );
        }
        // How the measure screen applies a physio grade.
        let mut confirmed = record(3, mmt_title(3));
        assert_eq!(confirmed.badge(), "Grade 3 / 5 (Fair)");
        confirmed.mmt_grade = 4;
        confirmed.mmt_title = mmt_title(4).into();
        confirmed.mmt_source = MmtSource::Physio;
        assert_eq!(confirmed.badge(), "Grade 4 / 5 (Good)");
    }
}
