//! Clinical vocabulary: sides, movements, muscles, MMT grading and the
//! anthropometric load estimates shared with the Flutter phone app.
//!
//! Estimated forces are gravitational/inertial loads of the arm segments, not
//! muscle force. Automatic MMT grades are suggestions for physiotherapist
//! review, not a validated clinical score.

use serde::{Deserialize, Serialize};

pub const STANDARD_GRAVITY: f64 = 9.80665;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Side {
    Left,
    #[default]
    Right,
}

impl Side {
    pub fn sign(self) -> f64 {
        match self {
            Side::Left => -1.0,
            Side::Right => 1.0,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Side::Left => "Left",
            Side::Right => "Right",
        }
    }

    pub fn from_key(key: &str) -> Option<Side> {
        match key.trim().to_ascii_lowercase().as_str() {
            "left" | "l" => Some(Side::Left),
            "right" | "r" => Some(Side::Right),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Movement {
    ShoulderAbduction,
    ElbowFlexion,
}

impl Movement {
    pub const ALL: [Movement; 2] = [Movement::ShoulderAbduction, Movement::ElbowFlexion];

    pub fn key(self) -> &'static str {
        match self {
            Movement::ShoulderAbduction => "shoulder_abduction",
            Movement::ElbowFlexion => "elbow_flexion",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Movement::ShoulderAbduction => "Shoulder abduction",
            Movement::ElbowFlexion => "Elbow flexion",
        }
    }

    /// Value of the phone app's `bodyPart` field.
    pub fn body_part(self) -> &'static str {
        match self {
            Movement::ShoulderAbduction => "Shoulder",
            Movement::ElbowFlexion => "Arm",
        }
    }

    pub fn from_key(key: &str) -> Option<Movement> {
        match key {
            "shoulder_abduction" => Some(Movement::ShoulderAbduction),
            "elbow_flexion" => Some(Movement::ElbowFlexion),
            _ => None,
        }
    }

    /// Records written by the phone app have no `movement`; infer it from `bodyPart`.
    pub fn infer(movement: Option<&str>, body_part: &str) -> Movement {
        movement.and_then(Movement::from_key).unwrap_or(
            if body_part.eq_ignore_ascii_case("shoulder") {
                Movement::ShoulderAbduction
            } else {
                Movement::ElbowFlexion
            },
        )
    }

    pub fn muscles(self) -> &'static [Muscle] {
        match self {
            Movement::ShoulderAbduction => &[Muscle::MiddleDeltoid, Muscle::Supraspinatus],
            Movement::ElbowFlexion => &[Muscle::BicepsBrachii],
        }
    }

    /// Typical full active range (AAOS reference values).
    pub fn normal_range_deg(self) -> f64 {
        match self {
            Movement::ShoulderAbduction => 180.0,
            Movement::ElbowFlexion => 150.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Muscle {
    MiddleDeltoid,
    Supraspinatus,
    AnteriorDeltoid,
    PectoralisMajor,
    Infraspinatus,
    BicepsBrachii,
}

impl Muscle {
    pub const ALL: [Muscle; 6] = [
        Muscle::MiddleDeltoid,
        Muscle::Supraspinatus,
        Muscle::AnteriorDeltoid,
        Muscle::PectoralisMajor,
        Muscle::Infraspinatus,
        Muscle::BicepsBrachii,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Muscle::MiddleDeltoid => "middle_deltoid",
            Muscle::Supraspinatus => "supraspinatus",
            Muscle::AnteriorDeltoid => "anterior_deltoid",
            Muscle::PectoralisMajor => "pectoralis_major_clavicular",
            Muscle::Infraspinatus => "infraspinatus",
            Muscle::BicepsBrachii => "biceps_brachii",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Muscle::MiddleDeltoid => "Middle deltoid",
            Muscle::Supraspinatus => "Supraspinatus",
            Muscle::AnteriorDeltoid => "Anterior deltoid",
            Muscle::PectoralisMajor => "Pectoralis major (clavicular)",
            Muscle::Infraspinatus => "Infraspinatus",
            Muscle::BicepsBrachii => "Biceps brachii",
        }
    }

    /// Measurement that produces an MMT grade for this muscle, if any.
    pub fn graded_by(self) -> Option<Movement> {
        Movement::ALL
            .into_iter()
            .find(|movement| movement.muscles().contains(&self))
    }
}

pub const MMT_TITLES: [&str; 6] = ["Zero", "Trace", "Poor", "Fair", "Good", "Normal"];

pub fn mmt_title(grade: u8) -> &'static str {
    MMT_TITLES[usize::from(grade.min(5))]
}

#[derive(Clone, Debug, PartialEq)]
pub struct MmtResult {
    pub grade: u8,
    pub title: String,
    pub rationale: String,
    /// Grade needs a physiotherapist decision (palpation for 0/1, resistance for 4/5).
    pub needs_physio_confirmation: bool,
}

impl MmtResult {
    fn new(grade: u8, rationale: String, needs_physio_confirmation: bool) -> Self {
        Self {
            grade,
            title: mmt_title(grade).to_string(),
            rationale,
            needs_physio_confirmation,
        }
    }

    pub fn badge(&self) -> String {
        format!("Grade {} / 5 ({})", self.grade, self.title)
    }
}

/// Elevation (deg) that counts as reaching the 90 deg test range, allowing for
/// sensor and placement tolerance.
pub const ABDUCTION_FULL_TEST_RANGE_DEG: f64 = 80.0;

/// Shoulder abduction against gravity (sitting): automatic grades 0-3 only.
pub fn grade_shoulder_abduction(
    peak_elevation_deg: f64,
    elbow_flexion_deg: Option<f64>,
    plane_deg: Option<f64>,
) -> MmtResult {
    let peak = if peak_elevation_deg.is_finite() {
        peak_elevation_deg.clamp(0.0, 180.0)
    } else {
        0.0
    };
    let percent = (peak / 90.0 * 100.0).min(100.0);
    let mut result = if peak < 5.0 {
        MmtResult::new(
            0,
            "No measurable abduction against gravity. Palpate the deltoid and supraspinatus to tell grade 0 from grade 1 (trace contraction).".into(),
            true,
        )
    } else if peak < 15.0 {
        MmtResult::new(
            1,
            format!("Flicker of abduction ({peak:.0} deg) without functional range."),
            false,
        )
    } else if peak < ABDUCTION_FULL_TEST_RANGE_DEG {
        MmtResult::new(
            2,
            format!(
                "Partial range against gravity: {peak:.0} deg of the 90 deg test range ({percent:.0}%)."
            ),
            false,
        )
    } else {
        MmtResult::new(
            3,
            format!(
                "Completes the test range against gravity ({peak:.0} deg). Grades 4-5 need manual resistance: physiotherapist to confirm."
            ),
            true,
        )
    };
    if let Some(elbow) = elbow_flexion_deg
        && elbow > 30.0
    {
        result.rationale.push_str(&format!(
            " Elbow was bent {elbow:.0} deg (shorter lever); repeat with the elbow straight for a valid test."
        ));
    }
    if let Some(plane) = plane_deg {
        if plane > 40.0 {
            result.rationale.push_str(&format!(
                " Arm drifted forward into flexion ({plane:.0} deg from the side plane)."
            ));
        } else if plane < -30.0 {
            result.rationale.push_str(&format!(
                " Arm drifted behind the body ({:.0} deg).",
                -plane
            ));
        }
    }
    result
}

/// Arm measurements used for load estimates (same inputs as the phone app).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anthropometrics {
    pub bicep_cm: f64,
    pub forearm_cm: f64,
    pub arm_length_cm: f64,
}

impl Default for Anthropometrics {
    fn default() -> Self {
        Self {
            bicep_cm: 29.0,
            forearm_cm: 24.5,
            arm_length_cm: 55.0,
        }
    }
}

impl Anthropometrics {
    pub fn validate(&self) -> Result<(), String> {
        let check = |v: f64, lo: f64, hi: f64, name: &str| {
            if v.is_finite() && v >= lo && v <= hi {
                Ok(())
            } else {
                Err(format!("Enter a {name} between {lo:.0} and {hi:.0} cm."))
            }
        };
        check(self.bicep_cm, 10.0, 65.0, "bicep circumference")?;
        check(self.forearm_cm, 10.0, 55.0, "forearm circumference")?;
        check(self.arm_length_cm, 15.0, 95.0, "arm length")
    }

    fn bicep_area_cm2(&self) -> f64 {
        self.bicep_cm * self.bicep_cm / (4.0 * std::f64::consts::PI)
    }

    fn forearm_area_cm2(&self) -> f64 {
        self.forearm_cm * self.forearm_cm / (4.0 * std::f64::consts::PI)
    }

    /// Frustum volume x soft-tissue density (1.05 g/cm^3).
    pub fn total_arm_mass_kg(&self) -> f64 {
        let (a1, a2) = (self.bicep_area_cm2(), self.forearm_area_cm2());
        let volume = self.arm_length_cm / 3.0 * (a1 + a2 + (a1 * a2).sqrt());
        volume * 1.05 / 1000.0
    }

    pub fn effective_moving_mass_kg(&self) -> f64 {
        let (a1, a2) = (self.bicep_area_cm2(), self.forearm_area_cm2());
        let sum = a1 + a2;
        if sum <= 0.0 {
            return 1.2;
        }
        let distal_share = (0.45 + 0.35 * (a2 / sum)).clamp(0.40, 0.75);
        (self.total_arm_mass_kg() * distal_share).clamp(0.25, 8.0)
    }

    /// Port of the phone app's `calculateForceNewtons` (elbow flexion model).
    pub fn elbow_force_newtons(&self, angle_deg: f64, accel_g: f64) -> f64 {
        let angle = if angle_deg.is_finite() {
            angle_deg.clamp(0.0, 180.0)
        } else {
            0.0
        };
        let accel = if accel_g.is_finite() && accel_g >= 0.0 {
            accel_g
        } else {
            0.0
        };
        let mass = self.effective_moving_mass_kg();
        let radians = angle.clamp(12.0, 160.0).to_radians();
        let posture = 0.55 + 0.45 * radians.sin().abs();
        let flexion_load = 1.0 + 0.38 * (angle / 145.0);
        let lever = (self.arm_length_cm / 55.0).clamp(0.55, 1.65);
        let bicep_leverage = (self.bicep_cm / 28.0).clamp(0.65, 1.60);
        let gravity = mass * STANDARD_GRAVITY * posture * flexion_load * lever;
        let dynamic = mass * accel * STANDARD_GRAVITY * lever * bicep_leverage;
        round1(gravity + dynamic)
    }

    /// Whole-arm weight component perpendicular to the arm at `elevation_deg`.
    pub fn shoulder_load_newtons(&self, elevation_deg: f64) -> f64 {
        let e = elevation_deg.clamp(0.0, 180.0).to_radians();
        round1(self.total_arm_mass_kg() * STANDARD_GRAVITY * e.sin())
    }

    /// Gravitational torque about the shoulder (centre of mass at 44% of arm length).
    pub fn shoulder_torque_nm(&self, elevation_deg: f64) -> f64 {
        round1(self.shoulder_load_newtons(elevation_deg) * self.arm_length_cm * 0.01 * 0.44)
    }
}

/// Default accelerometer intensity used by the phone app when only quaternions are streamed.
pub const PHONE_APP_DEFAULT_ACCEL_G: f64 = 0.22;

/// Port of the phone app's `MmtAssessment.evaluate` for elbow flexion.
pub fn grade_elbow_flexion(angle_deg: f64, force_n: f64, arm: &Anthropometrics) -> MmtResult {
    let angle = if angle_deg.is_finite() {
        angle_deg.clamp(0.0, 180.0)
    } else {
        0.0
    };
    let force = if force_n.is_finite() && force_n >= 0.0 {
        force_n
    } else {
        0.0
    };
    let baseline = arm.effective_moving_mass_kg() * STANDARD_GRAVITY;
    let ratio = if baseline > 0.0 {
        force / baseline
    } else {
        0.0
    };
    if angle < 10.0 && force < 2.0 {
        MmtResult::new(
            0,
            "No measurable joint excursion or active force output detected.".into(),
            true,
        )
    } else if angle < 35.0 || ratio < 0.45 {
        MmtResult::new(
            1,
            format!(
                "Trace muscle activation ({angle:.1} deg, {force:.1} N) without functional excursion."
            ),
            false,
        )
    } else if angle < 75.0 || ratio < 0.80 {
        MmtResult::new(
            2,
            format!(
                "Partial range of motion ({angle:.1} deg) with gravity-assisted force ({force:.1} N)."
            ),
            false,
        )
    } else if angle < 105.0 || ratio < 1.12 {
        MmtResult::new(
            3,
            format!(
                "Holds target position against gravity ({angle:.1} deg, {force:.1} N) without added resistance."
            ),
            false,
        )
    } else if angle < 128.0 || ratio < 1.38 {
        MmtResult::new(
            4,
            format!(
                "Full functional range ({angle:.1} deg) with moderate force capacity ({force:.1} N)."
            ),
            false,
        )
    } else {
        MmtResult::new(
            5,
            format!(
                "Full range of motion ({angle:.1} deg) with strong peak force output ({force:.1} N)."
            ),
            false,
        )
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Draft recovery guidance per MMT grade. Must be reviewed by a physiotherapist.
pub struct Guideline {
    pub focus: &'static str,
    pub activities: &'static [&'static str],
    pub next_goal: &'static str,
    pub patient_message: &'static str,
}

pub fn guideline(grade: u8) -> Guideline {
    match grade.min(5) {
        0 => Guideline {
            focus: "Elicit a first contraction",
            activities: &[
                "Caregiver-assisted passive range of motion, 10 slow repetitions, twice daily",
                "Supported positioning of the arm to protect the shoulder",
                "Mirror therapy or imagining the movement while the physio taps the muscle",
            ],
            next_goal: "Grade 1: a visible or palpable flicker of contraction",
            patient_message: "Your helper moves the arm for you. Watch and imagine lifting it.",
        },
        1 => Guideline {
            focus: "Turn the flicker into movement",
            activities: &[
                "Active-assisted lifts with the caregiver supporting the elbow",
                "Gravity-minimised slides: arm on a table with a towel, slide out to the side",
                "Short holds: try to keep the arm where the helper places it for 3 seconds",
            ],
            next_goal: "Grade 2: move through range with gravity minimised",
            patient_message: "Slide your arm on the table. Every small movement counts!",
        },
        2 => Guideline {
            focus: "Build range against gravity",
            activities: &[
                "Game Level 0 - draw the apple, small size, 3 apples per session",
                "Table slides progressing to lifting the arm off the table",
                "Active lifts out to the side as high as comfortable, 2 x 10",
            ],
            next_goal: "Grade 3: lift the arm out to the side to shoulder height (90 deg)",
            patient_message: "Draw apples in the air and lift a little higher each day.",
        },
        3 => Guideline {
            focus: "Control and endurance through full range",
            activities: &[
                "Game Levels 1-2 - PNF D1 and D2 fruit slicing, 8 repetitions",
                "Hold the arm at shoulder height for 5-10 seconds",
                "Slow lowering (eccentric control) from shoulder height",
            ],
            next_goal: "Grade 4: hold against light resistance (physiotherapist to test)",
            patient_message: "Great lifting! Slice the fruit along the diagonal and hold at the top.",
        },
        4 => Guideline {
            focus: "Progressive strengthening",
            activities: &[
                "Light resistance band in D1/D2 patterns as prescribed",
                "Functional reaching: shelves, combing hair, dressing",
                "Increase repetitions before increasing resistance",
            ],
            next_goal: "Grade 5: hold against strong resistance (physiotherapist to test)",
            patient_message: "Your arm is getting strong. Keep up daily practice.",
        },
        _ => Guideline {
            focus: "Maintain strength and function",
            activities: &[
                "Daily functional use of the arm",
                "Endurance sets in PNF patterns",
                "Keep the game streak going for coordination",
            ],
            next_goal: "Maintain grade 5 and full function",
            patient_message: "Excellent! Keep the arm active every day.",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shoulder_grades_follow_range_against_gravity() {
        assert_eq!(grade_shoulder_abduction(2.0, None, None).grade, 0);
        assert_eq!(grade_shoulder_abduction(10.0, None, None).grade, 1);
        assert_eq!(grade_shoulder_abduction(45.0, None, None).grade, 2);
        assert_eq!(grade_shoulder_abduction(79.9, None, None).grade, 2);
        let full = grade_shoulder_abduction(95.0, None, None);
        assert_eq!(full.grade, 3);
        assert!(full.needs_physio_confirmation);
        assert_eq!(grade_shoulder_abduction(f64::NAN, None, None).grade, 0);
    }

    #[test]
    fn shoulder_rationale_flags_compensation() {
        let r = grade_shoulder_abduction(60.0, Some(50.0), Some(55.0));
        assert!(r.rationale.contains("Elbow was bent 50"));
        assert!(r.rationale.contains("drifted forward"));
    }

    #[test]
    fn elbow_model_matches_phone_app_reference_values() {
        // Seeded patient "Ahmad Razak" in the phone app: 30.5 / 25.0 / 56.0 cm at 122 deg.
        let arm = Anthropometrics {
            bicep_cm: 30.5,
            forearm_cm: 25.0,
            arm_length_cm: 56.0,
        };
        let force = arm.elbow_force_newtons(122.0, 0.18);
        let expected = {
            let mass = arm.effective_moving_mass_kg();
            let rad = 122.0f64.to_radians();
            let g = mass
                * STANDARD_GRAVITY
                * (0.55 + 0.45 * rad.sin())
                * (1.0 + 0.38 * 122.0 / 145.0)
                * (56.0 / 55.0);
            let d = mass * 0.18 * STANDARD_GRAVITY * (56.0 / 55.0) * (30.5 / 28.0);
            ((g + d) * 10.0).round() / 10.0
        };
        assert_eq!(force, expected);
        assert!(grade_elbow_flexion(122.0, force, &arm).grade >= 3);
        assert_eq!(grade_elbow_flexion(5.0, 1.0, &arm).grade, 0);
    }

    #[test]
    fn shoulder_load_peaks_at_horizontal() {
        let arm = Anthropometrics::default();
        assert!(arm.shoulder_load_newtons(90.0) > arm.shoulder_load_newtons(45.0));
        assert_eq!(arm.shoulder_load_newtons(0.0), 0.0);
        assert!(arm.shoulder_torque_nm(90.0) > 0.0);
    }

    #[test]
    fn infers_movement_for_phone_records() {
        assert_eq!(Movement::infer(None, "Arm"), Movement::ElbowFlexion);
        assert_eq!(
            Movement::infer(None, "Shoulder"),
            Movement::ShoulderAbduction
        );
        assert_eq!(
            Movement::infer(Some("shoulder_abduction"), "Arm"),
            Movement::ShoulderAbduction
        );
    }

    #[test]
    fn muscles_map_to_grading_movements() {
        assert_eq!(
            Muscle::BicepsBrachii.graded_by(),
            Some(Movement::ElbowFlexion)
        );
        assert_eq!(
            Muscle::Supraspinatus.graded_by(),
            Some(Movement::ShoulderAbduction)
        );
        assert_eq!(Muscle::PectoralisMajor.graded_by(), None);
        assert_eq!(Muscle::Infraspinatus.key(), "infraspinatus");
    }

    #[test]
    fn validates_anthropometrics() {
        assert!(Anthropometrics::default().validate().is_ok());
        let bad = Anthropometrics {
            bicep_cm: 5.0,
            ..Anthropometrics::default()
        };
        assert!(bad.validate().is_err());
    }
}
