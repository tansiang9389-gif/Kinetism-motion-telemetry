//! Patient list, search and creation.

use crate::app::KinetismApp;
use crate::clinical::{Movement, Side};
use crate::model::{Patient, new_id};
use crate::theme;
use crate::ui::{self, card};
use eframe::egui::{self, RichText};

impl KinetismApp {
    pub(crate) fn patients_screen(&mut self, ui: &mut egui::Ui) {
        ui::eyebrow(ui, "Step 1");
        ui::title(ui, "Select patient");
        ui::subtitle(
            ui,
            "Patients and records are shared with the Kinetism phone app through Firestore.",
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.patients.search)
                    .hint_text("Search name or patient ID")
                    .desired_width(320.0),
            );
            if ui::primary(ui, "+ New patient").clicked() {
                self.patients.creating = true;
                self.patients.error = None;
            }
            if ui.button("Overall record log").clicked() {
                self.records.overall = true;
                self.screen = crate::app::Screen::Records;
            }
        });
        if self.patients.creating {
            ui.add_space(6.0);
            self.new_patient_form(ui);
        }
        ui.add_space(8.0);
        let query = self.patients.search.trim().to_lowercase();
        let patients: Vec<Patient> = self
            .store
            .patients
            .iter()
            .filter(|p| {
                query.is_empty()
                    || p.name.to_lowercase().contains(&query)
                    || p.code.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        if patients.is_empty() {
            card(ui, |ui| {
                ui.label(RichText::new("No patients found").strong());
                ui.label(ui::muted(if self.store.status().loading {
                    "Loading patients from Firestore…"
                } else {
                    "Create a new patient to start measuring."
                }));
            });
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for p in patients {
                    let records = self.store.records_for(&p.id);
                    let latest_shoulder = records
                        .iter()
                        .find(|r| r.movement == Movement::ShoulderAbduction)
                        .map(|r| r.mmt_grade);
                    let latest_elbow = records
                        .iter()
                        .find(|r| r.movement == Movement::ElbowFlexion)
                        .map(|r| r.mmt_grade);
                    let record_count = records.len();
                    let game_count = self.store.games_for(&p.id).len();
                    let clicked = card(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&p.name).size(19.0).strong());
                                ui.label(ui::small(format!(
                                    "{} · {} · {} records · {} games{}",
                                    p.code,
                                    p.affected_side.map_or("side not set".into(), |s| format!(
                                        "{} side",
                                        s.label()
                                    )),
                                    record_count,
                                    game_count,
                                    p.updated_at
                                        .map(|t| format!(
                                            " · updated {}",
                                            t.with_timezone(&chrono::Local).format("%d %b %H:%M")
                                        ))
                                        .unwrap_or_default()
                                )));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let open = ui::primary(ui, "Open").clicked();
                                ui::grade_badge(ui, latest_elbow, "Elbow");
                                ui::grade_badge(ui, latest_shoulder, "Shoulder");
                                open
                            })
                            .inner
                        })
                        .inner
                    });
                    if clicked {
                        self.select_patient(&p.id, true);
                    }
                    ui.add_space(4.0);
                }
            });
    }

    fn new_patient_form(&mut self, ui: &mut egui::Ui) {
        ui::tinted_card(ui, theme::ACCENT, |ui| {
            ui.label(RichText::new("New patient").strong());
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.patients.name)
                        .hint_text("Patient name")
                        .desired_width(240.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.patients.code)
                        .hint_text("Patient ID (optional)")
                        .desired_width(180.0),
                );
                ui.label("Affected side:");
                ui.selectable_value(&mut self.patients.side, Side::Left, "Left");
                ui.selectable_value(&mut self.patients.side, Side::Right, "Right");
            });
            if let Some(e) = &self.patients.error {
                ui.label(RichText::new(e).color(theme::ERROR));
            }
            ui.horizontal(|ui| {
                if ui::primary(ui, "Create & open").clicked() {
                    let name = self.patients.name.trim().to_string();
                    if name.len() < 2 {
                        self.patients.error = Some("Enter the patient's name.".into());
                    } else {
                        let code = if self.patients.code.trim().is_empty() {
                            format!("PT-{}", 101 + self.store.patients.len())
                        } else {
                            self.patients.code.trim().to_string()
                        };
                        let patient = Patient {
                            id: new_id("patient"),
                            name,
                            code,
                            arm: None,
                            affected_side: Some(self.patients.side),
                            updated_at: Some(chrono::Utc::now()),
                        };
                        let id = patient.id.clone();
                        self.store.upsert_patient(patient);
                        self.patients = Default::default();
                        self.select_patient(&id, true);
                        self.toast("Patient created and queued for Firestore.", false);
                    }
                }
                if ui.button("Cancel").clicked() {
                    self.patients.creating = false;
                }
            });
        });
    }
}
