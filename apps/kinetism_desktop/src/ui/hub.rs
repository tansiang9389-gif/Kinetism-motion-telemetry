//! Patient hub: Measure, Play, Progress and Record log.

use crate::app::{KinetismApp, MeasureStage, Screen};
use crate::clinical::{Movement, Side};
use crate::game::{best_score, is_unlocked, streak_days};
use crate::model::{GameLevel, UNLOCK_SCORE_PERCENT};
use crate::theme;
use crate::ui::{self, card};
use eframe::egui::{self, RichText};

impl KinetismApp {
    pub(crate) fn hub_screen(&mut self, ui: &mut egui::Ui) {
        let Some(patient) = self.current_patient() else {
            self.screen = Screen::Patients;
            return;
        };
        ui::eyebrow(ui, "Step 2 · Patient");
        ui.horizontal(|ui| {
            ui::title(ui, &patient.name);
            ui.add_space(12.0);
            ui.label(ui::muted(format!("ID {}", patient.code)));
            ui.add_space(12.0);
            ui.label("Affected side:");
            let mut side = patient.affected_side.unwrap_or(self.side());
            let before = side;
            ui.selectable_value(&mut side, Side::Left, "Left");
            ui.selectable_value(&mut side, Side::Right, "Right");
            if side != before {
                let mut updated = patient.clone();
                updated.affected_side = Some(side);
                updated.updated_at = Some(chrono::Utc::now());
                self.store.upsert_patient(updated);
                self.set_side(side);
            }
        });
        ui.add_space(6.0);

        let games = self.store.games_for(&patient.id);
        let streak = streak_days(&games, chrono::Local::now().date_naive());
        let records = self.store.records_for(&patient.id);
        let shoulder = records
            .iter()
            .find(|r| r.movement == Movement::ShoulderAbduction)
            .map(|r| r.mmt_grade);
        let elbow = records
            .iter()
            .find(|r| r.movement == Movement::ElbowFlexion)
            .map(|r| r.mmt_grade);
        let unlock = self.store.settings.physio_unlock_all;
        let level_status: Vec<(GameLevel, bool, Option<f64>)> = GameLevel::ALL
            .into_iter()
            .map(|l| (l, is_unlocked(&games, l, unlock), best_score(&games, l)))
            .collect();
        let record_count = records.len();
        drop(records);
        drop(games);

        ui.horizontal_wrapped(|ui| {
            ui::grade_badge(ui, shoulder, "Shoulder abduction MMT");
            ui::grade_badge(ui, elbow, "Elbow flexion MMT");
            ui::chip(
                ui,
                &format!("🔥 {streak}-day game streak"),
                if streak > 0 {
                    theme::WARN
                } else {
                    theme::FAINT
                },
            );
        });
        ui.add_space(10.0);

        let level_line: String = level_status
            .iter()
            .map(|(l, unlocked, best)| {
                format!(
                    "{} {} {}",
                    if *unlocked { "🔓" } else { "🔒" },
                    l.short(),
                    best.map_or("-".into(), |b| format!("{b:.0}%"))
                )
            })
            .collect::<Vec<_>>()
            .join("   ");
        let cards: [(&str, &str, String, &str, bool, Screen); 4] = [
            ("📐", "Measure", "Shoulder abduction or elbow flexion: calibrate, capture angle, load and an MMT suggestion.".into(), "Start measuring", true, Screen::Measure),
            ("🍎", "Play: Fruit PNF", format!("{level_line}\n{UNLOCK_SCORE_PERCENT:.0}% unlocks the next level."), "Play", true, Screen::Play),
            ("💪", "Progress on muscle", "Body map, daily MMT level, MMT and game trends with next-goal guidance.".into(), "View progress", false, Screen::Progress),
            ("🗂", "Record log", format!("{record_count} measurement records for this patient, plus game sessions."), "Open record log", false, Screen::Records),
        ];
        let per_row = if ui.available_width() > 1100.0 { 4 } else { 2 };
        let mut go: Option<Screen> = None;
        for row in cards.chunks(per_row) {
            ui.columns(per_row, |cols| {
                for ((icon, heading, body, button, primary, target), ui) in
                    row.iter().zip(cols.iter_mut())
                {
                    card(ui, |ui| {
                        ui.set_min_height(190.0);
                        ui.label(RichText::new(*icon).size(30.0));
                        ui.label(RichText::new(*heading).size(20.0).strong());
                        ui.label(ui::muted(body.as_str()));
                        ui.add_space(6.0);
                        let clicked = if *primary {
                            ui::primary(ui, button)
                        } else {
                            ui::secondary(ui, button)
                        }
                        .clicked();
                        if clicked {
                            go = Some(*target);
                        }
                    });
                }
            });
            ui.add_space(12.0);
        }
        if let Some(screen) = go {
            match screen {
                Screen::Measure => {
                    self.measure.stage = MeasureStage::Setup;
                    self.measure.error = None;
                }
                Screen::Records => self.records.overall = false,
                Screen::Play => {
                    self.play.game = None;
                    self.play.result = None;
                }
                _ => {}
            }
            self.screen = screen;
        }
    }
}
