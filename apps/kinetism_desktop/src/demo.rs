//! Demo mode (`KINETISM_DEMO=<scenario>`): synthetic offline data, scripted
//! simulator and optional PNG screenshot (`KINETISM_SCREENSHOT=<path>`).
//!
//! Scenarios: patients, hub, calibrate, measure, play, game0, game1, game2,
//! result, progress, progress-patient, records. Demo mode never syncs to
//! Firestore and uses a throwaway data folder.

use crate::app::{Audience, KinetismApp, MeasureStage, Screen};
use crate::clinical::{
    Anthropometrics, Movement, Side, grade_elbow_flexion, grade_shoulder_abduction,
};
use crate::game::pnf::Phase;
use crate::game::{Mode, Pt, dist, lerp, stars_for};
use crate::model::{GameLevel, GameSession, MeasurementRecord, MmtSource, Patient};
use crate::sim::SimInput;
use crate::tracker::CalStep;
use chrono::{Duration, Utc};
use eframe::egui;

const PATIENT_ID: &str = "patient-demo-synthetic";

fn scenario(app: &KinetismApp) -> String {
    app.demo
        .as_ref()
        .map(|d| d.scenario.clone())
        .unwrap_or_default()
}

fn needs_calibration(s: &str) -> bool {
    matches!(s, "measure" | "game0" | "game1" | "game2" | "result")
}

pub fn setup(app: &mut KinetismApp) {
    seed(app);
    app.select_patient(PATIENT_ID, false);
    let s = scenario(app);
    app.screen = match s.as_str() {
        "patients" => Screen::Patients,
        "calibrate" | "measure" => {
            app.measure.stage = MeasureStage::Calibrate;
            Screen::Measure
        }
        "play" | "game0" | "game1" | "game2" | "result" => Screen::Play,
        "progress" => Screen::Progress,
        "progress-patient" => {
            app.progress.audience = Audience::Patient;
            Screen::Progress
        }
        "records" => Screen::Records,
        _ => Screen::Hub,
    };
    if s == "calibrate" {
        app.tracker.start_rest_capture();
    }
}

fn seed(app: &mut KinetismApp) {
    let arm = Anthropometrics {
        bicep_cm: 27.5,
        forearm_cm: 23.0,
        arm_length_cm: 53.0,
    };
    let patient = Patient {
        id: PATIENT_ID.into(),
        name: "Demo Patient (synthetic)".into(),
        code: "PT-DEMO".into(),
        arm: Some(arm),
        affected_side: Some(Side::Right),
        updated_at: Some(Utc::now()),
    };
    app.store.patients = vec![
        patient.clone(),
        Patient {
            id: "patient-demo-2".into(),
            name: "Second Demo (synthetic)".into(),
            code: "PT-DEMO2".into(),
            arm: None,
            affected_side: Some(Side::Left),
            updated_at: Some(Utc::now() - Duration::days(3)),
        },
    ];
    let mut records = Vec::new();
    let shoulder_peaks = [
        22.0, 30.0, 34.0, 41.0, 48.0, 55.0, 61.0, 66.0, 74.0, 83.0, 88.0,
    ];
    for (i, peak) in shoulder_peaks.iter().enumerate() {
        let days_ago = (shoulder_peaks.len() - 1 - i) as i64;
        let mmt = grade_shoulder_abduction(*peak, Some(6.0), Some(8.0));
        records.push(MeasurementRecord {
            id: format!("rec-demo-s{i}"),
            patient_id: PATIENT_ID.into(),
            patient_name: patient.name.clone(),
            body_part: "Shoulder".into(),
            movement: Movement::ShoulderAbduction,
            side: Some(Side::Right),
            arm,
            angle_degrees: peak - 3.0,
            peak_angle_degrees: Some(*peak),
            force_newtons: arm.shoulder_load_newtons(*peak),
            torque_nm: Some(arm.shoulder_torque_nm(*peak)),
            mmt_grade: mmt.grade,
            mmt_title: mmt.title,
            mmt_rationale: mmt.rationale,
            mmt_source: MmtSource::Auto,
            auto_mmt_grade: Some(mmt.grade),
            attempt_number: i as u32 + 1,
            elbow_flexion_degrees: Some(6.0),
            plane_degrees: Some(8.0),
            recorded_at: Some(Utc::now() - Duration::days(days_ago) - Duration::hours(2)),
            source: "kinetism-desktop".into(),
        });
    }
    for (i, angle) in [70.0, 92.0, 108.0, 118.0].iter().enumerate() {
        let force = arm.elbow_force_newtons(*angle, 0.22);
        let mmt = grade_elbow_flexion(*angle, force, &arm);
        records.push(MeasurementRecord {
            id: format!("rec-demo-e{i}"),
            patient_id: PATIENT_ID.into(),
            patient_name: patient.name.clone(),
            body_part: "Arm".into(),
            movement: Movement::ElbowFlexion,
            side: Some(Side::Right),
            arm,
            angle_degrees: *angle,
            peak_angle_degrees: Some(*angle),
            force_newtons: force,
            torque_nm: None,
            mmt_grade: mmt.grade,
            mmt_title: mmt.title,
            mmt_rationale: mmt.rationale,
            mmt_source: MmtSource::Auto,
            auto_mmt_grade: Some(mmt.grade),
            attempt_number: i as u32 + 1,
            elbow_flexion_degrees: Some(*angle),
            plane_degrees: None,
            recorded_at: Some(Utc::now() - Duration::days(9 - 3 * i as i64) - Duration::hours(3)),
            source: "phone".into(),
        });
    }
    records.sort_by_key(|r| std::cmp::Reverse(r.recorded_at));
    app.store.records = records;
    let mut games = Vec::new();
    let plan: [(GameLevel, i64, f64, f64); 9] = [
        (GameLevel::AppleTrace, 8, 52.0, 48.0),
        (GameLevel::AppleTrace, 6, 64.0, 57.0),
        (GameLevel::AppleTrace, 5, 71.0, 63.0),
        (GameLevel::AppleTrace, 3, 82.0, 72.0),
        (GameLevel::PnfD1, 3, 66.0, 95.0),
        (GameLevel::PnfD1, 2, 78.0, 104.0),
        (GameLevel::AppleTrace, 1, 88.0, 80.0),
        (GameLevel::PnfD1, 1, 84.0, 112.0),
        (GameLevel::PnfD2, 0, 61.0, 118.0),
    ];
    for (i, (level, days_ago, score, elevation)) in plan.into_iter().enumerate() {
        games.push(GameSession {
            id: format!("game-demo-{i}"),
            patient_id: PATIENT_ID.into(),
            patient_name: patient.name.clone(),
            side: Side::Right,
            level,
            score_percent: score,
            points: (score * 6.0) as i64,
            stars: stars_for(score),
            reps: if level == GameLevel::AppleTrace { 3 } else { 8 },
            clean_reps: (score / 15.0) as u32,
            bombs_hit: u32::from(score < 70.0),
            max_combo: (score / 25.0) as u32,
            accuracy_percent: score - 4.0,
            completion_percent: 100.0,
            duration_seconds: 75.0,
            max_elevation_deg: elevation,
            recorded_at: Some(
                Utc::now()
                    - Duration::days(days_ago)
                    - Duration::hours(1)
                    - Duration::minutes(i as i64),
            ),
            source: "kinetism-desktop".into(),
        });
    }
    games.sort_by_key(|g| std::cmp::Reverse(g.recorded_at));
    app.store.games = games;
}

fn move_towards(from: Pt, to: Pt, max_step: f64) -> Pt {
    let d = dist(from, to);
    if d <= max_step {
        to
    } else {
        lerp(from, to, max_step / d)
    }
}

/// Scripted simulator motion for demos (calibration, measuring, playing).
pub fn autopilot(app: &mut KinetismApp, dt: f64) {
    let Some(demo) = app.demo.as_mut() else {
        return;
    };
    let s = demo.scenario.clone();
    demo.clock += dt;
    let clock = demo.clock;
    if needs_calibration(&s) && !app.tracker.is_calibrated() {
        demo.cal_timer += dt;
        match app.tracker.step {
            CalStep::Rest if demo.cal_timer > 0.4 => {
                app.sim.rest_pose();
                app.tracker.start_rest_capture();
            }
            CalStep::Forward => {
                app.sim.forward_pose();
                if demo.cal_timer > 1.8 {
                    app.tracker.start_forward_capture();
                }
            }
            _ => {}
        }
        return;
    }
    match s.as_str() {
        "calibrate" => {
            app.sim.rest_pose();
        }
        "measure" => {
            if app.screen == Screen::Measure && app.measure.stage != MeasureStage::Live {
                app.measure.stage = MeasureStage::Live;
                app.tracker.reset_peaks();
            }
            app.sim.input = SimInput::Sliders;
            app.sim.arm.elevation_deg = 47.0 + 45.0 * (clock * 0.7).sin().abs().max(0.05);
            app.sim.arm.plane_deg = 8.0;
            app.sim.arm.elbow_deg = 5.0;
        }
        "game0" | "game1" | "game2" | "result" => {
            let level = match s.as_str() {
                "game1" => GameLevel::PnfD1,
                "game2" => GameLevel::PnfD2,
                _ => GameLevel::AppleTrace,
            };
            if app.play.game.is_none() && app.play.result.is_none() {
                app.start_game(level);
            }
            let hand = app.tracker.hand.unwrap_or([0.3, -0.9]);
            let Some(game) = app.play.game.as_ref() else {
                return;
            };
            let goal = match &game.mode {
                Mode::Apple(round) => {
                    let t = &round.tracer;
                    if !t.started {
                        t.start_point()
                    } else {
                        t.path.points[(t.next + 3).min(t.path.points.len() - 1)]
                    }
                }
                Mode::Pnf(round) => match round.phase {
                    Phase::GoToStart | Phase::Return | Phase::Done => round.path.start,
                    Phase::Swipe => {
                        let nearest = (0..=50)
                            .map(|i| i as f64 / 50.0)
                            .min_by(|a, b| {
                                dist(round.path.point(*a), hand)
                                    .total_cmp(&dist(round.path.point(*b), hand))
                            })
                            .unwrap_or(0.0);
                        round.path.point((nearest + 0.12).min(1.0))
                    }
                },
            };
            let current = match app.sim.input {
                SimInput::HandTarget(p) => p,
                SimInput::Sliders => hand,
            };
            app.sim.input = SimInput::HandTarget(move_towards(current, goal, 0.95 * dt));
        }
        _ => {}
    }
}

fn save_png(path: &std::path::Path, image: &egui::ColorImage) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, image.size[0] as u32, image.size[1] as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
    let bytes: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|c| c.to_srgba_unmultiplied())
        .collect();
    writer
        .write_image_data(&bytes)
        .map_err(std::io::Error::other)
}

/// Capture a screenshot after the configured delay, then close the window.
pub fn tick(app: &mut KinetismApp, ctx: &egui::Context, _dt: f64) {
    let Some(demo) = app.demo.as_mut() else {
        return;
    };
    let Some(path) = demo.screenshot.clone() else {
        return;
    };
    ctx.request_repaint();
    if demo.requested || demo.started.elapsed().as_secs_f64() < demo.delay {
        return;
    }
    demo.requested = true;
    let ctx2 = ctx.clone();
    ctx.request_screenshot(move |image| {
        if let Err(e) = save_png(&path, &image) {
            eprintln!("screenshot failed: {e}");
        }
        ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
    });
}
