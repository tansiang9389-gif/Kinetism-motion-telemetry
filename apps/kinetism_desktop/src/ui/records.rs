//! Record log: measurements and game sessions, per patient or overall.

use crate::app::{KinetismApp, RecordsTab};
use crate::model::MmtSource;
use crate::theme;
use crate::ui::{self, card};
use chrono::Local;
use eframe::egui::{self, RichText, Vec2};

impl KinetismApp {
    pub(crate) fn records_screen(&mut self, ui: &mut egui::Ui) {
        let patient = self.current_patient();
        if patient.is_none() {
            self.records.overall = true;
        }
        ui::eyebrow(ui, "Record log");
        ui::title(
            ui,
            &if self.records.overall {
                "Overall record log".to_string()
            } else {
                format!(
                    "{} record log",
                    patient.as_ref().map_or("Patient", |p| p.name.as_str())
                )
            },
        );
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.records.tab,
                RecordsTab::Measurements,
                "Measurements",
            );
            ui.selectable_value(&mut self.records.tab, RecordsTab::Games, "Game sessions");
            ui.separator();
            if patient.is_some() {
                ui.selectable_value(&mut self.records.overall, false, "This patient");
            }
            ui.selectable_value(&mut self.records.overall, true, "All patients");
            ui.separator();
            ui.add(
                egui::TextEdit::singleline(&mut self.records.search)
                    .hint_text("Search name, movement, grade, level")
                    .desired_width(280.0),
            );
        });
        ui.add_space(6.0);
        let query = self.records.search.trim().to_lowercase();
        let patient_id = if self.records.overall {
            None
        } else {
            patient.map(|p| p.id)
        };
        card(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| match self.records.tab {
                    RecordsTab::Measurements => {
                        self.measurement_rows(ui, patient_id.as_deref(), &query)
                    }
                    RecordsTab::Games => self.game_rows(ui, patient_id.as_deref(), &query),
                });
        });
    }

    fn measurement_rows(&self, ui: &mut egui::Ui, patient_id: Option<&str>, query: &str) {
        let rows: Vec<_> = self
            .store
            .records
            .iter()
            .filter(|r| patient_id.is_none_or(|id| r.patient_id == id))
            .filter(|r| {
                query.is_empty()
                    || r.patient_name.to_lowercase().contains(query)
                    || r.movement.label().to_lowercase().contains(query)
                    || r.badge().to_lowercase().contains(query)
            })
            .collect();
        if rows.is_empty() {
            ui.label(ui::muted(
                "No measurement records yet. Start a measurement from the patient hub.",
            ));
            return;
        }
        egui::Grid::new("records")
            .striped(true)
            .num_columns(8)
            .spacing(Vec2::new(18.0, 8.0))
            .show(ui, |ui| {
                for h in [
                    "Date",
                    "Patient",
                    "Movement",
                    "Angle / peak",
                    "Load",
                    "MMT",
                    "Attempt",
                    "Source",
                ] {
                    ui.label(ui::small(h));
                }
                ui.end_row();
                for r in rows {
                    ui.label(r.recorded_at.map_or("-".into(), |t| {
                        t.with_timezone(&Local).format("%d %b %H:%M").to_string()
                    }));
                    ui.label(&r.patient_name);
                    ui.label(format!(
                        "{}{}",
                        r.movement.label(),
                        r.side
                            .map_or(String::new(), |s| format!(" ({})", s.label()))
                    ));
                    ui.label(match r.peak_angle_degrees {
                        Some(p) => format!("{:.1}° / {p:.1}°", r.angle_degrees),
                        None => format!("{:.1}°", r.angle_degrees),
                    });
                    ui.label(format!("{:.1} N", r.force_newtons));
                    ui::chip(
                        ui,
                        &format!("{} {}", r.mmt_grade, r.mmt_title),
                        theme::grade_color(r.mmt_grade),
                    )
                    .on_hover_text(&r.mmt_rationale);
                    ui.label(format!(
                        "#{}{}",
                        r.attempt_number,
                        if r.mmt_source == MmtSource::Physio {
                            " · physio"
                        } else {
                            ""
                        }
                    ));
                    // Local-only data (demo, KINETISM_OFFLINE) never syncs.
                    let pending = self.project.is_some() && self.store.is_record_pending(&r.id);
                    ui.label(
                        RichText::new(if pending {
                            "⟳ pending".to_string()
                        } else {
                            r.source.replace("kinetism-", "")
                        })
                        .size(13.0)
                        .color(if pending {
                            theme::WARN
                        } else {
                            theme::MUTED
                        }),
                    );
                    ui.end_row();
                }
            });
    }

    fn game_rows(&self, ui: &mut egui::Ui, patient_id: Option<&str>, query: &str) {
        let rows: Vec<_> = self
            .store
            .games
            .iter()
            .filter(|g| patient_id.is_none_or(|id| g.patient_id == id))
            .filter(|g| {
                query.is_empty()
                    || g.patient_name.to_lowercase().contains(query)
                    || g.level.title().to_lowercase().contains(query)
            })
            .collect();
        if rows.is_empty() {
            ui.label(ui::muted(
                "No game sessions yet. Play from the patient hub.",
            ));
            return;
        }
        egui::Grid::new("games")
            .striped(true)
            .num_columns(8)
            .spacing(Vec2::new(18.0, 8.0))
            .show(ui, |ui| {
                for h in [
                    "Date",
                    "Patient",
                    "Level",
                    "Score",
                    "Stars",
                    "Reps (clean)",
                    "Accuracy",
                    "Shoulder max",
                ] {
                    ui.label(ui::small(h));
                }
                ui.end_row();
                for g in rows {
                    ui.label(g.recorded_at.map_or("-".into(), |t| {
                        t.with_timezone(&Local).format("%d %b %H:%M").to_string()
                    }));
                    ui.label(&g.patient_name);
                    ui.label(format!("{} ({})", g.level.title(), g.side.label()));
                    ui.label(RichText::new(format!("{:.0}%", g.score_percent)).strong());
                    ui::stars(ui, g.stars, 16.0);
                    ui.label(format!("{} ({})", g.reps, g.clean_reps));
                    ui.label(format!("{:.0}%", g.accuracy_percent));
                    ui.label(format!("{:.0}°", g.max_elevation_deg));
                    ui.end_row();
                }
            });
    }
}
