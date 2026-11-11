//! Measure: setup, two-step calibration and live shoulder/elbow capture.

use crate::app::{KinetismApp, MeasureStage, Screen};
use crate::clinical::{
    Movement, PHONE_APP_DEFAULT_ACCEL_G, Side, grade_elbow_flexion, grade_shoulder_abduction,
    mmt_title,
};
use crate::model::{MeasurementRecord, MmtSource, SOURCE_DESKTOP, new_id};
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
                self.set_side(side);
            }
        });
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.label(RichText::new("Arm dimensions (for load estimates)").strong());
            ui.label(ui::small(
                "Same inputs as the phone app. Saved to the patient record.",
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
            let arm = self.arm();
            match arm.validate() {
                Err(e) => self.measure.error = Some(e),
                Ok(()) => {
                    self.measure.error = None;
                    if let Some(mut p) = self.current_patient()
                        && (p.arm != Some(arm) || p.affected_side != Some(self.side()))
                    {
                        p.arm = Some(arm);
                        p.affected_side = Some(self.side());
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
    }

    /// Two-step calibration. Returns true once calibrated.
    pub(crate) fn calibration_panel(&mut self, ui: &mut egui::Ui) -> bool {
        if self.tracker.is_calibrated() {
            return true;
        }
        let live = self.sensor.state().is_live();
        ui.label(ui::muted(format!(
            "{} arm. Seat the patient upright. Each step needs about one second of holding still.",
            self.side().label()
        )));
        if !live {
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
        let (current, peak) = match movement {
            Movement::ShoulderAbduction => {
                (pose.map(|p| p.elevation_deg), self.tracker.peak_elevation)
            }
            Movement::ElbowFlexion => (
                pose.and_then(|p| p.elbow_flexion_deg),
                self.tracker.peak_elbow,
            ),
        };
        let (mmt, force, torque) = match movement {
            Movement::ShoulderAbduction => (
                grade_shoulder_abduction(
                    peak,
                    pose.and_then(|p| p.elbow_flexion_deg),
                    pose.and_then(|p| p.plane_deg),
                ),
                arm.shoulder_load_newtons(current.unwrap_or(0.0)),
                Some(arm.shoulder_torque_nm(current.unwrap_or(0.0))),
            ),
            Movement::ElbowFlexion => {
                let f = arm.elbow_force_newtons(peak, PHONE_APP_DEFAULT_ACCEL_G);
                (
                    grade_elbow_flexion(peak, f, &arm),
                    arm.elbow_force_newtons(current.unwrap_or(0.0), PHONE_APP_DEFAULT_ACCEL_G),
                    None,
                )
            }
        };
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
        let stale = self
            .tracker
            .last_frame_at
            .is_none_or(|t| t.elapsed().as_secs_f32() > 1.5);
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
                            !stale && current.is_some(),
                            "● Record attempt",
                        )
                        .clicked();
                        if ui.button("Reset peak").clicked() {
                            self.tracker.reset_peaks();
                        }
                    });
                });
                ui.add_space(8.0);
                self.angle_trace(ui, movement);
            });
        });
        if record_clicked {
            self.record_attempt(movement, current.unwrap_or(0.0), peak, force, torque);
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

    fn record_attempt(
        &mut self,
        movement: Movement,
        angle: f64,
        peak: f64,
        force: f64,
        torque: Option<f64>,
    ) {
        let Some(patient) = self.current_patient() else {
            return;
        };
        let arm = self.arm();
        let pose = self.tracker.pose;
        let mmt = match movement {
            Movement::ShoulderAbduction => grade_shoulder_abduction(
                peak,
                pose.and_then(|p| p.elbow_flexion_deg),
                pose.and_then(|p| p.plane_deg),
            ),
            Movement::ElbowFlexion => grade_elbow_flexion(
                peak,
                arm.elbow_force_newtons(peak, PHONE_APP_DEFAULT_ACCEL_G),
                &arm,
            ),
        };
        let attempt = self
            .store
            .records_for(&patient.id)
            .iter()
            .filter(|r| r.movement == movement)
            .count() as u32
            + 1;
        let record = MeasurementRecord {
            id: new_id("rec"),
            patient_id: patient.id.clone(),
            patient_name: patient.name.clone(),
            body_part: movement.body_part().into(),
            movement,
            side: Some(self.side()),
            arm,
            angle_degrees: (angle * 10.0).round() / 10.0,
            peak_angle_degrees: Some((peak * 10.0).round() / 10.0),
            force_newtons: force,
            torque_nm: torque,
            mmt_grade: mmt.grade,
            mmt_title: mmt.title.clone(),
            mmt_rationale: mmt.rationale.clone(),
            mmt_source: MmtSource::Auto,
            auto_mmt_grade: Some(mmt.grade),
            attempt_number: attempt,
            elbow_flexion_degrees: pose
                .and_then(|p| p.elbow_flexion_deg)
                .map(|v| (v * 10.0).round() / 10.0),
            plane_degrees: pose
                .and_then(|p| p.plane_deg)
                .map(|v| (v * 10.0).round() / 10.0),
            recorded_at: Some(chrono::Utc::now()),
            source: SOURCE_DESKTOP.into(),
        };
        self.measure.last_record = Some(record.id.clone());
        self.store.add_record(record);
        self.tracker.reset_peaks();
        self.voice
            .say(&format!("Recorded. {:.0} degrees.", peak), true);
        self.toast(format!("Attempt #{attempt} saved · {}", mmt.badge()), false);
    }

    fn last_record_card(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.measure.last_record.clone() else {
            return;
        };
        let Some(record) = self.store.records.iter().find(|r| r.id == id).cloned() else {
            return;
        };
        ui.add_space(8.0);
        let mut new_grade: Option<u8> = None;
        ui::tinted_card(ui, theme::grade_color(record.mmt_grade), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("✓ Attempt #{} saved", record.attempt_number)).strong(),
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
                if self.store.is_record_pending(&record.id) {
                    ui::chip(ui, "waiting for Firestore", theme::WARN);
                } else if self.project.is_some() {
                    ui::chip(ui, "saved to Firestore", theme::GOOD);
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

const AUTO_MARKER: &str = " | Auto suggestion ";

/// The automatic rationale, also when the record was already physio-graded.
fn auto_rationale(r: &MeasurementRecord) -> &str {
    r.mmt_rationale
        .split_once(AUTO_MARKER)
        .and_then(|(_, rest)| rest.split_once(": "))
        .map_or(r.mmt_rationale.as_str(), |(_, base)| base)
}
