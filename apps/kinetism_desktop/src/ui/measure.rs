//! Measure: setup, two-step calibration and live shoulder/elbow capture.

use crate::app::{KinetismApp, MeasureStage, Screen};
use crate::clinical::{
    Anthropometrics, MmtResult, Movement, PHONE_APP_DEFAULT_ACCEL_G, Side, grade_elbow_flexion,
    grade_shoulder_abduction, mmt_title,
};
use crate::kinematics::ArmPose;
use crate::model::{MeasurementRecord, MmtSource, Patient, SOURCE_DESKTOP, new_id};
use crate::theme;
use crate::tracker::CalStep;
use crate::ui::{self, card, figures};
use eframe::egui::{self, RichText, Vec2};

impl KinetismApp {
    pub(crate) fn measure_screen(&mut self, ui: &mut egui::Ui) {
        if self.current_patient().is_none() {
            self.screen = Screen::Patients;
            return;
        }
        match self.measure.stage {
            MeasureStage::Setup => self.measure_setup(ui),
            MeasureStage::Calibrate => {
                ui::eyebrow(ui, "Step 4 · Calibrate");
                ui::title(ui, "Calibrate the sensors");
                if self.calibration_panel(ui) {
                    self.measure.stage = MeasureStage::Live;
                    self.tracker.reset_peaks();
                }
            }
            MeasureStage::Live => self.measure_live(ui),
        }
    }

    fn measure_setup(&mut self, ui: &mut egui::Ui) {
        ui::eyebrow(ui, "Step 3 · Measure");
        ui::title(ui, "What are you measuring?");
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            for movement in Movement::ALL {
                let selected = self.measure.movement == movement;
                let subtitle = match movement {
                    Movement::ShoulderAbduction => {
                        "Middle deltoid + supraspinatus · upper-arm sensor"
                    }
                    Movement::ElbowFlexion => "Biceps brachii · both sensors",
                };
                let r = ui.add_sized(
                    Vec2::new(340.0, 64.0),
                    egui::Button::selectable(
                        selected,
                        RichText::new(format!("{}\n{}", movement.label(), subtitle)).size(15.0),
                    ),
                );
                if r.clicked() {
                    self.measure.movement = movement;
                }
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Arm being measured:");
            let mut side = self.side();
            ui.selectable_value(&mut side, Side::Left, "Left");
            ui.selectable_value(&mut side, Side::Right, "Right");
            if side != self.side() {
                self.choose_measured_arm(side);
            }
            if let Some(affected) = self.current_patient().and_then(|p| p.affected_side)
                && affected != self.side()
            {
                ui.label(ui::small(format!(
                    "Baseline on the other arm: the affected side stays {}.",
                    affected.label()
                )));
            }
        });
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.label(RichText::new("Arm dimensions (for load estimates)").strong());
            ui.label(ui::small(
                "Same inputs as the phone app. The affected arm's dimensions are saved with the patient.",
            ));
            egui::Grid::new("arm_dims")
                .num_columns(3)
                .spacing(Vec2::new(12.0, 8.0))
                .show(ui, |ui| {
                    for (label, value, hint) in [
                        (
                            "Circumference of bicep (cm)",
                            &mut self.measure.bicep,
                            "mid-upper arm",
                        ),
                        (
                            "Circumference of forearm (cm)",
                            &mut self.measure.forearm,
                            "widest proximal forearm",
                        ),
                        (
                            "Length of arm (cm)",
                            &mut self.measure.length,
                            "acromion to wrist styloid",
                        ),
                    ] {
                        ui.label(label);
                        ui.add(egui::TextEdit::singleline(value).desired_width(90.0));
                        ui.label(ui::small(hint));
                        ui.end_row();
                    }
                });
            if let Some(e) = &self.measure.error {
                ui.label(RichText::new(e).color(theme::ERROR));
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Sensor placement:").strong());
            ui.label(ui::muted(
                "master (Physio_A) on the lateral upper arm, slave on the back of the forearm. Patient seated upright.",
            ));
        });
        ui.add_space(8.0);
        if ui::primary(ui, "Continue").clicked() {
            self.finish_setup();
        }
    }

    /// Switches the arm being measured. Selecting the affected arm brings its
    /// saved sizes back into the form (as opening the patient does), so sizes
    /// typed for the other arm can never be saved over them. Selecting the
    /// other arm keeps the form as it is, to be edited for that arm.
    fn choose_measured_arm(&mut self, side: Side) {
        self.set_side(side);
        if let Some(p) = self.current_patient()
            && p.affected_side == Some(side)
        {
            self.measure.show_arm(p.arm.unwrap_or_default());
        }
    }

    /// Continue: saves what setup may change on the patient, then calibrates
    /// or goes straight to measuring.
    fn finish_setup(&mut self) {
        let arm = self.arm();
        match arm.validate() {
            Err(e) => self.measure.error = Some(e),
            Ok(()) => {
                self.measure.error = None;
                // Live reads the form: keep the checked sizes there, not a
                // fallback to saved sizes that a later pull could change.
                if self.measure.typed_arm().is_none() {
                    self.measure.show_arm(arm);
                }
                if let Some(mut p) = self
                    .current_patient()
                    .and_then(|p| patient_after_setup(&p, self.side(), arm))
                {
                    p.updated_at = Some(chrono::Utc::now());
                    self.store.upsert_patient(p);
                }
                self.measure.stage = if self.tracker.is_calibrated() {
                    MeasureStage::Live
                } else {
                    MeasureStage::Calibrate
                };
                self.tracker.reset_peaks();
            }
        }
    }

    /// Two-step calibration. Returns true once calibrated.
    pub(crate) fn calibration_panel(&mut self, ui: &mut egui::Ui) -> bool {
        if self.tracker.is_calibrated() {
            return true;
        }
        let stats = self.sensor.stats();
        let live = self.sensor.state().is_live() && (stats.packets == 0 || stats.upper_present);
        ui.label(ui::muted(format!(
            "{} arm. Seat the patient upright. Each step needs about one second of holding still.",
            self.side().label()
        )));
        if stats.packets > 0 && !stats.upper_present {
            ui::tinted_card(ui, theme::WARN, |ui| {
                ui.label(
                    RichText::new(
                        "The upper-arm (master) sensor sends no orientation, so calibration cannot start. Its IMU is not reporting.",
                    )
                    .color(theme::WARN),
                );
            });
        } else if !live {
            ui::tinted_card(ui, theme::WARN, |ui| {
                ui.label(
                    RichText::new(
                        "Connect a sensor (top right: Bluetooth, Wi-Fi or Simulator) to calibrate.",
                    )
                    .color(theme::WARN),
                );
            });
        }
        ui.add_space(6.0);
        let step = self.tracker.step;
        let mut done = false;
        ui.horizontal(|ui| {
            for n in [1u8, 2] {
                let active = matches!((n, step), (1, CalStep::Rest | CalStep::CapturingRest) | (2, CalStep::Forward | CalStep::CapturingForward));
                ui.vertical(|ui| {
                    figures::calibration_pose(ui, Vec2::new(300.0, 200.0), n, active);
                    let completed = n == 1 && matches!(step, CalStep::Forward | CalStep::CapturingForward);
                    if completed {
                        ui.label(RichText::new("✓ Captured").color(theme::GOOD));
                    } else if active {
                        let capturing = matches!(step, CalStep::CapturingRest | CalStep::CapturingForward);
                        if capturing {
                            ui.add(egui::ProgressBar::new(self.tracker.progress()).desired_width(300.0).text("Hold still…").fill(theme::ACCENT));
                            if self.tracker.moving_warning {
                                ui.label(RichText::new("Movement detected - restarting. Hold still.").color(theme::WARN));
                            }
                        } else {
                            let label = if n == 1 { "Capture rest pose" } else { "Capture forward pose" };
                            if ui::primary_enabled(ui, live, label).clicked() {
                                if n == 1 {
                                    self.tracker.start_rest_capture();
                                    self.voice.say("Relax your arm by your side and hold still.", true);
                                } else {
                                    self.tracker.start_forward_capture();
                                    self.voice.say("Lift your arm forward, elbow straight, and hold.", true);
                                }
                            }
                            if n == 2 && ui.button("Skip (elevation only)").on_hover_text("Uses the sensor's mounting to estimate the heading. Elevation stays accurate; the plane check and game are less precise.").clicked() {
                                self.tracker.skip_forward_pose();
                                done = true;
                            }
                        }
                    }
                });
            }
        });
        let text = match step {
            CalStep::Rest | CalStep::CapturingRest => {
                "Step 1: arm relaxed and hanging straight down by the side, elbow straight, palm facing the thigh."
            }
            _ => {
                "Step 2: raise the arm straight forward (at least 25°, about 60° is ideal), elbow straight. A helper may support the arm."
            }
        };
        ui.label(text);
        if let Some(msg) = &self.tracker.message {
            ui.label(RichText::new(msg).color(theme::WARN));
        }
        if self.tracker.is_calibrated() {
            self.voice.say("Calibration complete.", true);
            done = true;
        }
        done
    }

    fn measure_live(&mut self, ui: &mut egui::Ui) {
        let movement = self.measure.movement;
        let arm = self.arm();
        let pose = self.tracker.pose;
        let (current, peak, peak_pose) = match movement {
            Movement::ShoulderAbduction => (
                pose.map(|p| p.elevation_deg),
                self.tracker.peak_elevation,
                self.tracker.peak_pose,
            ),
            Movement::ElbowFlexion => (
                pose.and_then(|p| p.elbow_flexion_deg),
                self.tracker.peak_elbow,
                self.tracker.peak_elbow_pose,
            ),
        };
        let angle = current.unwrap_or(0.0);
        let (force, torque) = match movement {
            Movement::ShoulderAbduction => (
                arm.shoulder_load_newtons(angle),
                Some(arm.shoulder_torque_nm(angle)),
            ),
            Movement::ElbowFlexion => (
                arm.elbow_force_newtons(angle, PHONE_APP_DEFAULT_ACCEL_G),
                None,
            ),
        };
        let attempt = Attempt {
            movement,
            side: self.side(),
            arm,
            angle,
            force,
            torque,
            peak,
            peak_pose,
        };
        let mmt = attempt.grade();
        let practice = self.practice_only();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui::eyebrow(ui, "Step 5 · Live measurement");
                ui::title(
                    ui,
                    &format!("{} · {} arm", movement.label(), self.side().label()),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Recalibrate").clicked() {
                    self.tracker.recalibrate();
                    self.measure.stage = MeasureStage::Calibrate;
                }
                if ui.button("Change movement").clicked() {
                    self.measure.stage = MeasureStage::Setup;
                }
            });
        });
        let stale = !self.tracker.has_live_data();
        if stale {
            ui::tinted_card(ui, theme::WARN, |ui| {
                ui.label(
                    RichText::new(
                        "No live sensor data. Connect a sensor (top right) or start the Simulator.",
                    )
                    .color(theme::WARN),
                );
            });
        }
        if movement == Movement::ElbowFlexion && self.tracker.forearm_missing() {
            ui::tinted_card(ui, theme::WARN, |ui| {
                ui.label(RichText::new("Elbow flexion needs the forearm (slave) sensor. Check that it is on and synced to the master.").color(theme::WARN));
            });
        }
        ui.add_space(6.0);
        let mut record_clicked = false;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                figures::measure_figure(
                    ui,
                    Vec2::new(420.0, 380.0),
                    pose.as_ref(),
                    self.side(),
                    movement,
                );
            });
            ui.vertical(|ui| {
                card(ui, |ui| {
                    ui.set_min_width(460.0);
                    ui.horizontal(|ui| {
                        ui::metric(
                            ui,
                            "Current angle",
                            &current.map_or("--".into(), |a| format!("{a:.1}°")),
                            theme::ACCENT,
                        );
                        ui.add_space(24.0);
                        ui::metric(ui, "Attempt peak", &format!("{peak:.1}°"), theme::TEXT);
                        ui.add_space(24.0);
                        ui::metric(
                            ui,
                            "Normal range",
                            &format!("0-{:.0}°", movement.normal_range_deg()),
                            theme::MUTED,
                        );
                    });
                    ui.horizontal(|ui| {
                        ui::metric(
                            ui,
                            if movement == Movement::ShoulderAbduction {
                                "Arm load (est.)"
                            } else {
                                "Force (phone model)"
                            },
                            &format!("{force:.1} N"),
                            theme::TEXT,
                        );
                        if let Some(t) = torque {
                            ui.add_space(24.0);
                            ui::metric(
                                ui,
                                "Shoulder torque (est.)",
                                &format!("{t:.1} N·m"),
                                theme::TEXT,
                            );
                        }
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("MMT suggestion:").strong());
                        ui::chip(ui, &mmt.badge(), theme::grade_color(mmt.grade));
                    });
                    ui.label(ui::small(&mmt.rationale));
                    if movement == Movement::ShoulderAbduction
                        && let Some(p) = pose
                    {
                        let mut warnings = Vec::new();
                        if let Some(e) = p.elbow_flexion_deg
                            && e > 30.0
                        {
                            warnings.push(format!("Straighten the elbow ({e:.0}° bent)"));
                        }
                        if let Some(plane) = p.plane_deg {
                            if plane > 40.0 {
                                warnings.push(format!(
                                    "Arm drifting forward ({plane:.0}°): keep it out to the side"
                                ));
                            } else if plane < -30.0 {
                                warnings.push("Arm moving behind the body".into());
                            }
                        }
                        for w in warnings {
                            ui.label(RichText::new(format!("⚠ {w}")).color(theme::WARN));
                        }
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        record_clicked = ui::primary_enabled(
                            ui,
                            !stale && current.is_some() && !practice,
                            "● Record attempt",
                        )
                        .clicked();
                        if ui.button("Reset peak").clicked() {
                            self.tracker.reset_peaks();
                        }
                    });
                    if practice {
                        ui.label(
                            RichText::new("Simulator: practice only, not saved").color(theme::WARN),
                        );
                    }
                });
                ui.add_space(8.0);
                self.angle_trace(ui, movement);
            });
        });
        if record_clicked {
            self.record_attempt(&attempt);
        }
        self.last_record_card(ui);
    }

    fn angle_trace(&mut self, ui: &mut egui::Ui, movement: Movement) {
        let now = self.tracker.now_seconds();
        let pts: Vec<(f64, f64)> = self
            .tracker
            .history
            .iter()
            .filter_map(|(t, e, el)| {
                let v = match movement {
                    Movement::ShoulderAbduction => Some(*e),
                    Movement::ElbowFlexion => *el,
                };
                v.map(|v| (t - now, v))
            })
            .collect();
        ui.label(ui::small("Last 15 seconds"));
        ui::line_chart(
            ui,
            &ui::Chart {
                size: Vec2::new(490.0, 150.0),
                x_range: (-15.0, 0.0),
                y_range: (0.0, 180.0),
                y_ticks: &[0.0, 45.0, 90.0, 135.0, 180.0],
                y_unit: "°",
                threshold: (movement == Movement::ShoulderAbduction).then_some((
                    90.0,
                    theme::GOOD,
                    "90° test range",
                )),
            },
            &[(pts, theme::ACCENT)],
        );
    }

    /// Saves the attempt for the current patient. Simulator data never becomes
    /// a patient record (demo mode excepted).
    fn record_attempt(&mut self, attempt: &Attempt) {
        let Some(patient) = self.current_patient() else {
            return;
        };
        if self.practice_only() {
            return;
        }
        let number = self
            .store
            .records_for(&patient.id)
            .iter()
            .filter(|r| r.movement == attempt.movement)
            .count() as u32
            + 1;
        let record = attempt.record(&patient, number);
        let badge = record.badge();
        self.measure.last_record = Some(record.id.clone());
        self.store.add_record(record);
        self.tracker.reset_peaks();
        self.voice
            .say(&format!("Recorded. {:.0} degrees.", attempt.peak), true);
        self.toast(format!("Attempt #{number} saved · {badge}"), false);
    }

    fn last_record_card(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.measure.last_record.clone() else {
            return;
        };
        let Some(patient) = self.current_patient() else {
            return;
        };
        let Some(record) = self.store.patient_record(&patient.id, &id).cloned() else {
            return;
        };
        ui.add_space(8.0);
        let mut new_grade: Option<u8> = None;
        ui::tinted_card(ui, theme::grade_color(record.mmt_grade), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "✓ Attempt #{} saved for {}",
                        record.attempt_number, record.patient_name
                    ))
                    .strong(),
                );
                ui.label(ui::small(format!(
                    "{:.1}° at record · peak {:.1}° · {:.1} N · {}{}",
                    record.angle_degrees,
                    record.peak_angle_degrees.unwrap_or(record.angle_degrees),
                    record.force_newtons,
                    record.badge(),
                    if record.mmt_source == MmtSource::Physio {
                        " (physio-confirmed)"
                    } else {
                        " (auto)"
                    }
                )));
                // Local-only data (demo, KINETISM_OFFLINE) never syncs.
                if self.project.is_some() {
                    if self.store.is_record_pending(&record.id) {
                        ui::chip(ui, "waiting for Firestore", theme::WARN);
                    } else {
                        ui::chip(ui, "saved to Firestore", theme::GOOD);
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                let needs = record.movement == Movement::ShoulderAbduction
                    && record.auto_mmt_grade == Some(3);
                ui.label(
                    RichText::new(if needs {
                        "Grade 3 reached. Test against manual resistance, then confirm:"
                    } else {
                        "Physiotherapist grade:"
                    })
                    .color(theme::TEXT),
                );
                for g in 0..=5u8 {
                    let selected = record.mmt_grade == g && record.mmt_source == MmtSource::Physio;
                    let text = RichText::new(format!("{g} {}", mmt_title(g))).color(if selected {
                        theme::BG
                    } else {
                        theme::grade_color(g)
                    });
                    let mut button = egui::Button::new(text);
                    if selected {
                        button = button.fill(theme::grade_color(g));
                    }
                    if ui.add(button).clicked() {
                        new_grade = Some(g);
                    }
                }
            });
        });
        if let Some(g) = new_grade {
            let auto = record.auto_mmt_grade.unwrap_or(record.mmt_grade);
            let mut updated = record.clone();
            updated.mmt_grade = g;
            updated.mmt_title = mmt_title(g).into();
            updated.mmt_source = MmtSource::Physio;
            updated.auto_mmt_grade = Some(auto);
            updated.mmt_rationale = format!(
                "Physiotherapist grade {g} ({}).{AUTO_MARKER}{auto}/5: {}",
                mmt_title(g),
                auto_rationale(&record)
            );
            self.store.add_record(updated);
            self.toast(format!("Grade {g} confirmed by physiotherapist."), false);
        }
    }
}

/// One captured attempt, before it becomes a record.
struct Attempt {
    movement: Movement,
    side: Side,
    arm: Anthropometrics,
    /// Angle when Record is pressed, with the load at that angle.
    angle: f64,
    force: f64,
    torque: Option<f64>,
    /// Highest angle of the attempt and the arm's posture at that moment.
    peak: f64,
    peak_pose: Option<ArmPose>,
}

impl Attempt {
    /// Automatic grade, judged at the peak: its angle and its posture.
    fn grade(&self) -> MmtResult {
        match self.movement {
            Movement::ShoulderAbduction => grade_shoulder_abduction(
                self.peak,
                self.peak_pose.and_then(|p| p.elbow_flexion_deg),
                self.peak_pose.and_then(|p| p.plane_deg),
            ),
            Movement::ElbowFlexion => grade_elbow_flexion(
                self.peak,
                self.arm
                    .elbow_force_newtons(self.peak, PHONE_APP_DEFAULT_ACCEL_G),
                &self.arm,
            ),
        }
    }

    /// The record saved as attempt `number`; elbow and plane describe the peak.
    fn record(&self, patient: &Patient, number: u32) -> MeasurementRecord {
        let mmt = self.grade();
        let round1 = |v: f64| (v * 10.0).round() / 10.0;
        MeasurementRecord {
            id: new_id("rec"),
            patient_id: patient.id.clone(),
            patient_name: patient.name.clone(),
            body_part: self.movement.body_part().into(),
            movement: self.movement,
            side: Some(self.side),
            arm: self.arm,
            angle_degrees: round1(self.angle),
            peak_angle_degrees: Some(round1(self.peak)),
            force_newtons: self.force,
            torque_nm: self.torque,
            mmt_grade: mmt.grade,
            mmt_title: mmt.title,
            mmt_rationale: mmt.rationale,
            mmt_source: MmtSource::Auto,
            auto_mmt_grade: Some(mmt.grade),
            attempt_number: number,
            elbow_flexion_degrees: self.peak_pose.and_then(|p| p.elbow_flexion_deg).map(round1),
            plane_degrees: self.peak_pose.and_then(|p| p.plane_deg).map(round1),
            recorded_at: Some(chrono::Utc::now()),
            source: SOURCE_DESKTOP.into(),
        }
    }
}

/// What Continue saves to the patient, or `None` if nothing changes. The
/// measured side only becomes the affected side when none is set (the hub
/// changes it), and arm dimensions are only saved for the affected arm or
/// when none are known: a baseline on the healthy arm overwrites neither.
fn patient_after_setup(patient: &Patient, measured: Side, arm: Anthropometrics) -> Option<Patient> {
    let mut updated = patient.clone();
    let affected = *updated.affected_side.get_or_insert(measured);
    if measured == affected || updated.arm.is_none() {
        updated.arm = Some(arm);
    }
    (updated != *patient).then_some(updated)
}

const AUTO_MARKER: &str = " | Auto suggestion ";

/// The automatic rationale, also when the record was already physio-graded.
fn auto_rationale(r: &MeasurementRecord) -> &str {
    r.mmt_rationale
        .split_once(AUTO_MARKER)
        .and_then(|(_, rest)| rest.split_once(": "))
        .map_or(r.mmt_rationale.as_str(), |(_, base)| base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{calibrate_simulator, calibrate_wifi, demo, test_app, test_dir, wifi};
    use crate::kinematics::{SAMPLES_PER_POSE, SimArm};
    use crate::sensor::TimedFrame;
    use crate::sensor::frame::SensorFrame;
    use crate::tracker::ArmTracker;
    use std::time::Instant;

    fn record(rationale: &str) -> MeasurementRecord {
        MeasurementRecord {
            id: "rec-1".into(),
            patient_id: "p1".into(),
            patient_name: "Pat".into(),
            body_part: "Shoulder".into(),
            movement: Movement::ShoulderAbduction,
            side: Some(Side::Right),
            arm: Anthropometrics::default(),
            angle_degrees: 82.0,
            peak_angle_degrees: Some(85.0),
            force_newtons: 30.0,
            torque_nm: Some(7.0),
            mmt_grade: 3,
            mmt_title: mmt_title(3).into(),
            mmt_rationale: rationale.into(),
            mmt_source: MmtSource::Auto,
            auto_mmt_grade: Some(3),
            attempt_number: 1,
            elbow_flexion_degrees: None,
            plane_degrees: None,
            recorded_at: None,
            source: SOURCE_DESKTOP.into(),
        }
    }

    #[test]
    fn auto_rationale_is_not_nested_by_repeated_physio_grades() {
        let auto = grade_shoulder_abduction(85.0, None, None).rationale;
        assert!(
            auto.contains(": "),
            "the grade 3 rationale itself contains a colon: {auto}"
        );
        let mut r = record(&auto);
        assert_eq!(
            auto_rationale(&r),
            auto,
            "auto-graded records keep their text"
        );
        // Same format as the physiotherapist grade buttons in `last_record_card`.
        for g in [4u8, 5, 2] {
            r.mmt_rationale = format!(
                "Physiotherapist grade {g} ({}).{AUTO_MARKER}3/5: {}",
                mmt_title(g),
                auto_rationale(&r)
            );
            assert_eq!(
                auto_rationale(&r),
                auto,
                "re-grading to {g} keeps the original suggestion"
            );
        }
        assert_eq!(
            r.mmt_rationale,
            format!("Physiotherapist grade 2 (Poor).{AUTO_MARKER}3/5: {auto}"),
            "only the latest physiotherapist grade is kept"
        );
        let phone = record("Entered on the phone");
        assert_eq!(auto_rationale(&phone), "Entered on the phone");
    }

    fn patient(arm: Option<Anthropometrics>, affected_side: Option<Side>) -> Patient {
        Patient {
            id: "p1".into(),
            name: "Pat".into(),
            code: "PT-1".into(),
            arm,
            affected_side,
            updated_at: None,
        }
    }

    #[test]
    fn a_baseline_on_the_healthy_arm_keeps_the_affected_side_and_its_dimensions() {
        let affected_arm = Anthropometrics {
            bicep_cm: 31.0,
            forearm_cm: 26.0,
            arm_length_cm: 57.0,
        };
        let healthy_arm = Anthropometrics::default();
        let stroke = patient(Some(affected_arm), Some(Side::Left));
        assert_eq!(
            patient_after_setup(&stroke, Side::Right, healthy_arm),
            None,
            "measuring the healthy arm changes nothing on the patient"
        );
        assert_eq!(
            patient_after_setup(&stroke, Side::Left, affected_arm),
            None,
            "unchanged dimensions are not re-saved"
        );
        let remeasured =
            patient_after_setup(&stroke, Side::Left, healthy_arm).expect("new affected-arm sizes");
        assert_eq!(
            (remeasured.affected_side, remeasured.arm),
            (Some(Side::Left), Some(healthy_arm))
        );

        let new = patient(None, None);
        let first = patient_after_setup(&new, Side::Right, healthy_arm).expect("first setup");
        assert_eq!(
            (first.affected_side, first.arm),
            (Some(Side::Right), Some(healthy_arm)),
            "the first measured side becomes the affected side"
        );
        let no_sizes = patient(None, Some(Side::Left));
        let baseline = patient_after_setup(&no_sizes, Side::Right, healthy_arm)
            .expect("sizes better than none");
        assert_eq!(
            (baseline.affected_side, baseline.arm),
            (Some(Side::Left), Some(healthy_arm))
        );
    }

    fn feed(tracker: &mut ArmTracker, sim: &SimArm, n: usize) {
        let (upper, forearm) = sim.quats();
        let frames: Vec<TimedFrame> = (0..n)
            .map(|_| TimedFrame {
                frame: SensorFrame {
                    upper: Some(upper),
                    forearm: Some(forearm),
                },
                at: Instant::now(),
            })
            .collect();
        tracker.feed(&frames);
    }

    fn calibrated(sim: &mut SimArm) -> ArmTracker {
        let mut tracker = ArmTracker::new(sim.side);
        tracker.start_rest_capture();
        feed(&mut tracker, sim, SAMPLES_PER_POSE);
        (sim.elevation_deg, sim.plane_deg) = (60.0, 90.0);
        tracker.start_forward_capture();
        feed(&mut tracker, sim, SAMPLES_PER_POSE);
        assert!(tracker.is_calibrated());
        tracker.reset_peaks();
        tracker
    }

    /// Attempt as the Live screen builds it from the tracker.
    fn attempt(tracker: &ArmTracker, movement: Movement) -> Attempt {
        let pose = tracker.pose.expect("live pose");
        let (angle, peak, peak_pose) = match movement {
            Movement::ShoulderAbduction => (
                pose.elevation_deg,
                tracker.peak_elevation,
                tracker.peak_pose,
            ),
            Movement::ElbowFlexion => (
                pose.elbow_flexion_deg.expect("elbow angle"),
                tracker.peak_elbow,
                tracker.peak_elbow_pose,
            ),
        };
        Attempt {
            movement,
            side: tracker.side,
            arm: Anthropometrics::default(),
            angle,
            force: 0.0,
            torque: None,
            peak,
            peak_pose,
        }
    }

    #[test]
    fn a_forward_swing_is_graded_and_saved_with_the_posture_of_its_peak() {
        let mut sim = SimArm::new(Side::Right);
        let mut tracker = calibrated(&mut sim);
        // Swing forward to 95 deg in the sagittal plane, then lower the arm to the side.
        for (elevation, plane) in [(95.0, 80.0), (40.0, 0.0)] {
            (sim.elevation_deg, sim.plane_deg) = (elevation, plane);
            feed(&mut tracker, &sim, 1);
        }
        let shoulder = attempt(&tracker, Movement::ShoulderAbduction);
        let near = |v: Option<f64>, expected: f64| v.is_some_and(|v| (v - expected).abs() < 0.5);
        assert!(
            near(Some(shoulder.angle), 40.0) && near(Some(shoulder.peak), 95.0),
            "pressed at 40 deg after a 95 deg peak"
        );
        let record = shoulder.record(&patient(None, Some(Side::Right)), 1);
        assert_eq!(record.mmt_grade, 3, "graded on the peak");
        assert!(
            record
                .mmt_rationale
                .contains("Arm drifted forward into flexion (80 deg"),
            "the peak's forward drift is flagged: {}",
            record.mmt_rationale
        );
        assert!(
            near(record.plane_degrees, 80.0) && near(record.peak_angle_degrees, 95.0),
            "the stored plane is the peak's: {:?}",
            record.plane_degrees
        );
        let at_press = tracker.pose.and_then(|p| p.plane_deg);
        assert!(
            near(at_press, 0.0)
                && !grade_shoulder_abduction(shoulder.peak, None, at_press)
                    .rationale
                    .contains("drifted"),
            "the posture when Record is pressed would hide the swing"
        );

        // Elbow flexion: curl to 100 deg with the arm at the side, then lift and relax.
        for (elevation, plane, elbow) in [(30.0, 0.0, 100.0), (50.0, 60.0, 10.0)] {
            (sim.elevation_deg, sim.plane_deg, sim.elbow_deg) = (elevation, plane, elbow);
            feed(&mut tracker, &sim, 1);
        }
        let elbow =
            attempt(&tracker, Movement::ElbowFlexion).record(&patient(None, Some(Side::Right)), 1);
        assert!(
            near(elbow.peak_angle_degrees, 100.0)
                && near(elbow.elbow_flexion_degrees, 100.0)
                && near(elbow.plane_degrees, 0.0),
            "elbow records describe their own peak: {elbow:?}"
        );
    }

    #[test]
    fn simulator_attempts_are_practice_only_outside_demo_mode() {
        let dir = test_dir("practice");
        let mut app = test_app(&dir);
        app.store.upsert_patient(patient(None, Some(Side::Right)));
        app.select_patient("p1", false);
        calibrate_simulator(&mut app);
        (app.sim.arm.elevation_deg, app.sim.arm.plane_deg) = (85.0, 0.0);
        app.advance(0.1);
        let simulated = attempt(&app.tracker, Movement::ShoulderAbduction);

        assert!(app.practice_only());
        app.record_attempt(&simulated);
        assert!(
            app.store.records.is_empty() && app.measure.last_record.is_none(),
            "simulated attempts are not saved or synced"
        );

        app.demo = Some(demo("hub"));
        assert!(
            !app.practice_only(),
            "demo mode saves to its throwaway store"
        );
        app.record_attempt(&simulated);
        assert_eq!(app.store.records.len(), 1);

        app.demo = None;
        let mut arm = SimArm::new(Side::Right);
        calibrate_wifi(&mut app, &mut arm);
        (arm.elevation_deg, arm.plane_deg) = (85.0, 0.0);
        wifi(&mut app, &arm, 1, 0.05);
        let measured = attempt(&app.tracker, Movement::ShoulderAbduction);
        assert!((measured.peak - 85.0).abs() < 0.5, "{}", measured.peak);
        app.record_attempt(&measured);
        assert_eq!(
            app.store.records.len(),
            2,
            "a real sensor's attempt is saved"
        );
        assert_eq!(
            app.store.records[0].id,
            app.measure.last_record.clone().unwrap_or_default()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn form(app: &KinetismApp) -> [String; 3] {
        [
            app.measure.bicep.clone(),
            app.measure.forearm.clone(),
            app.measure.length.clone(),
        ]
    }

    #[test]
    fn going_back_to_the_affected_arm_restores_its_saved_sizes() {
        let dir = test_dir("form-side");
        let mut app = test_app(&dir);
        let saved = Anthropometrics {
            bicep_cm: 31.0,
            forearm_cm: 26.0,
            arm_length_cm: 57.0,
        };
        app.store
            .upsert_patient(patient(Some(saved), Some(Side::Left)));
        app.select_patient("p1", false);
        let saved_form = ["31.0", "26.0", "57.0"].map(String::from);
        assert_eq!(form(&app), saved_form);

        app.choose_measured_arm(Side::Right);
        assert_eq!(
            form(&app),
            saved_form,
            "the other arm starts from the form as it was"
        );
        app.measure.show_arm(Anthropometrics {
            bicep_cm: 35.0,
            forearm_cm: 29.0,
            arm_length_cm: 60.0,
        });
        app.choose_measured_arm(Side::Left);
        assert_eq!(
            form(&app),
            saved_form,
            "the affected arm's saved sizes come back"
        );
        let queued = app.store.pending_count();
        app.finish_setup();
        assert_eq!(app.measure.stage, MeasureStage::Calibrate);
        assert_eq!(
            (
                app.store.patient("p1").and_then(|p| p.arm),
                app.store.pending_count()
            ),
            (Some(saved), queued),
            "the healthy arm's sizes are never saved over them"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn phone_sizes_outside_the_form_ranges_must_be_fixed_not_replaced() {
        let dir = test_dir("form-range");
        let mut app = test_app(&dir);
        let phone = Anthropometrics {
            bicep_cm: 30.0,
            forearm_cm: 25.0,
            arm_length_cm: 96.0,
        };
        app.store
            .upsert_patient(patient(Some(phone), Some(Side::Right)));
        app.select_patient("p1", false);
        assert_eq!(app.measure.length, "96.0", "the phone's value is shown");
        app.finish_setup();
        assert!(
            app.measure
                .error
                .as_deref()
                .is_some_and(|e| e.contains("arm length")),
            "{:?}",
            app.measure.error
        );
        assert_eq!(app.measure.stage, MeasureStage::Setup);
        assert_eq!(
            app.store.patient("p1").and_then(|p| p.arm),
            Some(phone),
            "no defaults written over it"
        );

        // Continue with a cleared field falls back to the saved sizes. Live
        // keeps those checked sizes even if a pull later changes the patient's.
        let fixed = Anthropometrics {
            arm_length_cm: 56.0,
            ..phone
        };
        app.measure.show_arm(fixed);
        app.finish_setup();
        app.measure.stage = MeasureStage::Setup;
        app.measure.bicep.clear();
        app.finish_setup();
        assert_eq!(
            app.measure.typed_arm(),
            Some(fixed),
            "the checked sizes fill the form"
        );
        if let Some(p) = app.store.patients.iter_mut().find(|p| p.id == "p1") {
            p.arm = Some(Anthropometrics {
                bicep_cm: 1e155,
                ..fixed
            });
        }
        assert_eq!(app.arm(), fixed, "Live never reads the pulled sizes");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
