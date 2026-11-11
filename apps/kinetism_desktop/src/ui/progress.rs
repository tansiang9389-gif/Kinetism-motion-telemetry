//! Progress on muscle: body map, daily MMT level, trends and guidance.

use crate::app::{Audience, KinetismApp, Screen};
use crate::clinical::{Muscle, guideline, mmt_title};
use crate::game::streak_days;
use crate::model::{GameLevel, GameSession, MeasurementRecord, MmtSource, UNLOCK_SCORE_PERCENT};
use crate::theme;
use crate::ui::{self, card, figures};
use chrono::{Local, NaiveDate};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Vec2,
};
use std::collections::BTreeMap;

const GRID_DAYS: i64 = 14;
const TREND_DAYS: i64 = 30;

fn local_day(r: &MeasurementRecord) -> Option<NaiveDate> {
    r.recorded_at.map(|t| t.with_timezone(&Local).date_naive())
}

/// Best MMT grade per local day for a muscle (the grade demonstrated that day).
fn daily_grades(records: &[&MeasurementRecord], muscle: Muscle) -> BTreeMap<NaiveDate, u8> {
    let mut out = BTreeMap::new();
    for r in records.iter().filter(|r| r.muscles().contains(&muscle)) {
        if let Some(day) = local_day(r) {
            let e = out.entry(day).or_insert(r.mmt_grade);
            *e = (*e).max(r.mmt_grade);
        }
    }
    out
}

fn game_days(games: &[&GameSession], muscle: Muscle) -> BTreeMap<NaiveDate, u32> {
    let mut out = BTreeMap::new();
    for g in games.iter().filter(|g| g.level.muscles().contains(&muscle)) {
        if let Some(t) = g.recorded_at {
            *out.entry(t.with_timezone(&Local).date_naive()).or_insert(0) += 1;
        }
    }
    out
}

struct MuscleSummary {
    muscle: Muscle,
    latest: Option<(u8, NaiveDate, MmtSource)>,
    previous_day: Option<u8>,
    daily: BTreeMap<NaiveDate, u8>,
    games: BTreeMap<NaiveDate, u32>,
}

impl KinetismApp {
    pub(crate) fn progress_screen(&mut self, ui: &mut egui::Ui) {
        let Some(patient) = self.current_patient() else {
            self.screen = Screen::Patients;
            return;
        };
        let today = Local::now().date_naive();
        let side = self.side();
        let records: Vec<MeasurementRecord> = self
            .store
            .records_for(&patient.id)
            .into_iter()
            .filter(|r| r.side.is_none_or(|s| s == side))
            .cloned()
            .collect();
        let games: Vec<GameSession> = self
            .store
            .games_for(&patient.id)
            .into_iter()
            .cloned()
            .collect();
        let record_refs: Vec<&MeasurementRecord> = records.iter().collect();
        let game_refs: Vec<&GameSession> = games.iter().collect();
        let summaries: Vec<MuscleSummary> = Muscle::ALL
            .into_iter()
            .map(|muscle| {
                let daily = daily_grades(&record_refs, muscle);
                let latest_record = record_refs.iter().find(|r| r.muscles().contains(&muscle));
                let latest = latest_record
                    .and_then(|r| local_day(r).map(|d| (r.mmt_grade, d, r.mmt_source)));
                let previous_day =
                    latest.and_then(|(_, d, _)| daily.range(..d).next_back().map(|(_, g)| *g));
                MuscleSummary {
                    muscle,
                    latest,
                    previous_day,
                    daily,
                    games: game_days(&game_refs, muscle),
                }
            })
            .collect();
        let grades: BTreeMap<Muscle, Option<u8>> = summaries
            .iter()
            .map(|s| (s.muscle, s.latest.map(|l| l.0)))
            .collect();
        let streak = streak_days(&game_refs, today);

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui::eyebrow(ui, "Progress on muscle");
                ui::title(ui, &format!("{} · {} side", patient.name, side.label()));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.selectable_value(
                    &mut self.progress.audience,
                    Audience::Physio,
                    "🩺 Physio view",
                );
                ui.selectable_value(
                    &mut self.progress.audience,
                    Audience::Patient,
                    "😊 Patient view",
                );
            });
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| match self.progress.audience {
                Audience::Patient => {
                    self.patient_progress(ui, &summaries, &grades, streak, &game_refs)
                }
                Audience::Physio => {
                    self.physio_progress(ui, &summaries, &grades, today, &game_refs)
                }
            });
    }

    fn patient_progress(
        &mut self,
        ui: &mut egui::Ui,
        summaries: &[MuscleSummary],
        grades: &BTreeMap<Muscle, Option<u8>>,
        streak: u32,
        games: &[&GameSession],
    ) {
        ui.horizontal_wrapped(|ui| {
            for s in summaries.iter().filter(|s| s.muscle.graded_by().is_some()) {
                let grade = s.latest.map(|l| l.0);
                let color = grade.map_or(theme::NO_DATA, theme::grade_color);
                ui::tinted_card(ui, color, |ui| {
                    ui.set_min_width(250.0);
                    ui.label(ui::small(format!(
                        "Today's MMT level · {}",
                        s.muscle.label()
                    )));
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(grade.map_or("-".into(), |g| g.to_string()))
                                .size(54.0)
                                .strong()
                                .color(color),
                        );
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(grade.map_or("Not measured", mmt_title))
                                    .size(20.0)
                                    .strong(),
                            );
                            let change = match (grade, s.previous_day) {
                                (Some(g), Some(p)) if g > p => format!("▲ up from {p}"),
                                (Some(g), Some(p)) if g < p => format!("▼ down from {p}"),
                                (Some(_), Some(_)) => "= same as last time".into(),
                                _ => "first measurement".into(),
                            };
                            ui.label(ui::muted(change));
                        });
                    });
                });
            }
            ui::tinted_card(ui, theme::WARN, |ui| {
                ui.set_min_width(200.0);
                ui.label(ui::small("Game streak"));
                ui.label(
                    RichText::new(format!("🔥 {streak}"))
                        .size(46.0)
                        .strong()
                        .color(theme::WARN),
                );
                ui.label(ui::muted(if streak > 0 {
                    "days in a row - keep it up!"
                } else {
                    "play today to start a streak"
                }));
            });
        });
        ui.add_space(8.0);
        ui.horizontal_top(|ui| {
            figures::body_map(ui, Vec2::new(520.0, 380.0), grades, self.side(), None);
            ui.vertical(|ui| {
                let focus = summaries
                    .iter()
                    .filter(|s| s.muscle.graded_by().is_some())
                    .min_by_key(|s| s.latest.map_or(0, |l| l.0))
                    .map(|s| (s.muscle, s.latest.map_or(0, |l| l.0)));
                if let Some((muscle, grade)) = focus {
                    let g = guideline(grade);
                    card(ui, |ui| {
                        ui.set_max_width(460.0);
                        ui.label(ui::small(format!("Your next goal · {}", muscle.label())));
                        ui.label(
                            RichText::new(g.next_goal)
                                .size(20.0)
                                .strong()
                                .color(theme::ACCENT),
                        );
                        ui.label(RichText::new(g.patient_message).size(17.0));
                    });
                }
                ui.add_space(8.0);
                if let Some(last) = games.first() {
                    card(ui, |ui| {
                        ui.label(ui::small("Last game"));
                        ui.label(RichText::new(last.level.title()).strong());
                        ui::stars(ui, last.stars, 30.0);
                        ui.label(ui::muted(format!(
                            "{:.0}% · shoulder up to {:.0}°",
                            last.score_percent, last.max_elevation_deg
                        )));
                    });
                }
            });
        });
    }

    fn physio_progress(
        &mut self,
        ui: &mut egui::Ui,
        summaries: &[MuscleSummary],
        grades: &BTreeMap<Muscle, Option<u8>>,
        today: NaiveDate,
        games: &[&GameSession],
    ) {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                let (_, clicked) = figures::body_map(ui, Vec2::new(440.0, 330.0), grades, self.side(), Some(self.progress.muscle));
                if let Some(m) = clicked {
                    self.progress.muscle = m;
                }
                ui.horizontal_wrapped(|ui| {
                    ui.set_max_width(440.0);
                    for g in 0..=5u8 {
                        ui::chip(ui, &format!("{g} {}", mmt_title(g)), theme::grade_color(g));
                    }
                    ui::chip(ui, "no data", theme::FAINT);
                });
                ui.label(ui::small("Click a muscle to see its trend and guidance."));
            });
            ui.vertical(|ui| {
                ui.label(RichText::new("Daily MMT level (best grade each day, last 14 days)").strong());
                daily_grid(ui, summaries, today);
                ui.label(ui::small("Numbers = MMT grade that day. Dots = PNF game sessions training the muscle (no MMT test)."));
            });
        });
        ui.add_space(10.0);
        let muscle = self.progress.muscle;
        let summary = summaries.iter().find(|s| s.muscle == muscle);
        ui.horizontal_top(|ui| {
            card(ui, |ui| {
                ui.label(RichText::new(format!("MMT trend · {} (daily best, {TREND_DAYS} days)", muscle.label())).strong());
                let pts: Vec<(f64, f64)> = summary
                    .map(|s| {
                        s.daily
                            .iter()
                            .map(|(d, g)| ((*d - today).num_days() as f64, f64::from(*g)))
                            .filter(|(x, _)| *x >= -(TREND_DAYS as f64))
                            .collect()
                    })
                    .unwrap_or_default();
                if muscle.graded_by().is_none() {
                    ui.label(ui::muted("Trained in the PNF game only - no MMT test is defined for it in this prototype."));
                }
                let goal = summary.and_then(|s| s.latest).map(|l| f64::from((l.0 + 1).min(5)));
                ui::line_chart(
                    ui,
                    &ui::Chart {
                        size: Vec2::new(500.0, 190.0),
                        x_range: (-(TREND_DAYS as f64), 0.0),
                        y_range: (0.0, 5.0),
                        y_ticks: &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
                        y_unit: "",
                        threshold: goal.map(|g| (g, theme::GOOD, "next goal")),
                    },
                    &[(pts, theme::ACCENT)],
                );
                ui.label(ui::small("x: days before today"));
            });
            card(ui, |ui| {
                ui.label(RichText::new("Game score trend").strong());
                let colors = [Color32::from_rgb(0xFF, 0x6B, 0x6B), Color32::from_rgb(0x6B, 0xD3, 0x6B), Color32::from_rgb(0xFF, 0xD5, 0x4F)];
                let series: Vec<(Vec<(f64, f64)>, Color32)> = GameLevel::ALL
                    .into_iter()
                    .zip(colors)
                    .map(|(level, color)| {
                        let mut pts: Vec<(f64, f64)> = games
                            .iter()
                            .filter(|g| g.level == level)
                            .filter_map(|g| {
                                g.recorded_at.map(|t| {
                                    let dt = t.with_timezone(&Local) - Local::now();
                                    (dt.num_minutes() as f64 / 1440.0, g.score_percent)
                                })
                            })
                            .filter(|(x, _)| *x >= -(TREND_DAYS as f64))
                            .collect();
                        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
                        (pts, color)
                    })
                    .collect();
                ui::line_chart(
                    ui,
                    &ui::Chart {
                        size: Vec2::new(500.0, 190.0),
                        x_range: (-(TREND_DAYS as f64), 0.0),
                        y_range: (0.0, 100.0),
                        y_ticks: &[0.0, 25.0, 50.0, 75.0, 100.0],
                        y_unit: "%",
                        threshold: Some((UNLOCK_SCORE_PERCENT, theme::GOOD, "unlock 80%")),
                    },
                    &series,
                );
                ui.horizontal(|ui| {
                    for (level, color) in GameLevel::ALL.into_iter().zip(colors) {
                        ui::chip(ui, level.short(), color);
                    }
                });
            });
        });
        ui.add_space(10.0);
        let grade = summary.and_then(|s| s.latest).map_or(0, |l| l.0);
        let g = guideline(grade);
        ui::tinted_card(ui, theme::ACCENT, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "Recovery guideline · {} · current grade {grade} ({})",
                        muscle.label(),
                        mmt_title(grade)
                    ))
                    .strong(),
                );
                ui::chip(ui, "DRAFT - physiotherapist to review", theme::WARN);
            });
            ui.label(RichText::new(format!("Focus: {}", g.focus)).color(theme::TEXT));
            for a in g.activities {
                ui.label(format!("•  {a}"));
            }
            ui.label(RichText::new(format!("Next goal: {}", g.next_goal)).color(theme::ACCENT));
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(RichText::new("Muscle summary").strong());
            egui::Grid::new("muscle_table")
                .striped(true)
                .num_columns(5)
                .spacing(Vec2::new(24.0, 6.0))
                .show(ui, |ui| {
                    for h in [
                        "Muscle",
                        "Latest MMT",
                        "Graded by",
                        "Measured",
                        "Game sessions (30 d)",
                    ] {
                        ui.label(ui::small(h));
                    }
                    ui.end_row();
                    for s in summaries {
                        ui.label(s.muscle.label());
                        match s.latest {
                            Some((g, _, source)) => {
                                ui::chip(
                                    ui,
                                    &format!(
                                        "{g} {} · {}",
                                        mmt_title(g),
                                        if source == MmtSource::Physio {
                                            "physio"
                                        } else {
                                            "auto"
                                        }
                                    ),
                                    theme::grade_color(g),
                                );
                            }
                            None => {
                                ui.label(ui::muted("-"));
                            }
                        }
                        ui.label(ui::muted(
                            s.muscle.graded_by().map_or("PNF game only", |m| m.label()),
                        ));
                        ui.label(ui::muted(
                            s.latest
                                .map_or("-".into(), |(_, d, _)| d.format("%d %b").to_string()),
                        ));
                        let recent: u32 = s
                            .games
                            .iter()
                            .filter(|(d, _)| (**d - today).num_days() >= -TREND_DAYS)
                            .map(|(_, n)| *n)
                            .sum();
                        ui.label(ui::muted(recent.to_string()));
                        ui.end_row();
                    }
                });
        });
    }
}

fn daily_grid(ui: &mut egui::Ui, summaries: &[MuscleSummary], today: NaiveDate) {
    let label_w = 200.0;
    let cell = Vec2::new(34.0, 28.0);
    let size = Vec2::new(
        label_w + cell.x * GRID_DAYS as f32,
        22.0 + cell.y * summaries.len() as f32,
    );
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter_at(rect);
    for i in 0..GRID_DAYS {
        let day = today - chrono::Duration::days(GRID_DAYS - 1 - i);
        let x = rect.left() + label_w + cell.x * (i as f32 + 0.5);
        p.text(
            Pos2::new(x, rect.top() + 9.0),
            Align2::CENTER_CENTER,
            if day == today {
                "today".to_string()
            } else {
                day.format("%d").to_string()
            },
            FontId::proportional(11.0),
            if day == today {
                theme::ACCENT
            } else {
                theme::FAINT
            },
        );
    }
    for (row, s) in summaries.iter().enumerate() {
        let y = rect.top() + 22.0 + cell.y * row as f32;
        p.text(
            Pos2::new(rect.left(), y + cell.y * 0.5),
            Align2::LEFT_CENTER,
            s.muscle.label(),
            FontId::proportional(13.0),
            theme::TEXT,
        );
        for i in 0..GRID_DAYS {
            let day = today - chrono::Duration::days(GRID_DAYS - 1 - i);
            let r = Rect::from_min_size(
                Pos2::new(rect.left() + label_w + cell.x * i as f32, y),
                cell,
            )
            .shrink(2.0);
            match s.daily.get(&day) {
                Some(g) => {
                    p.rect_filled(r, CornerRadius::same(5), theme::grade_color(*g));
                    p.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        g.to_string(),
                        FontId::proportional(14.0),
                        theme::BG,
                    );
                }
                None => {
                    p.rect_filled(r, CornerRadius::same(5), theme::DEEP);
                }
            }
            if let Some(n) = s.games.get(&day) {
                for k in 0..(*n).min(3) {
                    p.circle_filled(
                        Pos2::new(r.left() + 6.0 + k as f32 * 6.0, r.bottom() - 5.0),
                        2.5,
                        Color32::WHITE,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clinical::{Anthropometrics, Movement, Side};
    use chrono::{DateTime, TimeZone, Utc};

    fn at(day: NaiveDate, hour: u32) -> DateTime<Utc> {
        Local
            .from_local_datetime(&day.and_hms_opt(hour, 0, 0).expect("valid time"))
            .single()
            .expect("unambiguous local time")
            .with_timezone(&Utc)
    }

    fn record(movement: Movement, grade: u8, when: Option<DateTime<Utc>>) -> MeasurementRecord {
        MeasurementRecord {
            id: crate::model::new_id("rec"),
            patient_id: "p1".into(),
            patient_name: "Pat".into(),
            body_part: movement.body_part().into(),
            movement,
            side: Some(Side::Right),
            arm: Anthropometrics::default(),
            angle_degrees: 50.0,
            peak_angle_degrees: None,
            force_newtons: 10.0,
            torque_nm: None,
            mmt_grade: grade,
            mmt_title: mmt_title(grade).into(),
            mmt_rationale: String::new(),
            mmt_source: MmtSource::Auto,
            auto_mmt_grade: Some(grade),
            attempt_number: 1,
            elbow_flexion_degrees: None,
            plane_degrees: None,
            recorded_at: when,
            source: "test".into(),
        }
    }

    fn game(level: GameLevel, when: Option<DateTime<Utc>>) -> GameSession {
        GameSession {
            id: crate::model::new_id("game"),
            patient_id: "p1".into(),
            patient_name: "Pat".into(),
            side: Side::Right,
            level,
            score_percent: 70.0,
            points: 0,
            stars: 1,
            reps: 8,
            clean_reps: 0,
            bombs_hit: 0,
            max_combo: 0,
            accuracy_percent: 70.0,
            completion_percent: 100.0,
            duration_seconds: 60.0,
            max_elevation_deg: 90.0,
            recorded_at: when,
            source: "test".into(),
        }
    }

    #[test]
    fn daily_grades_keep_the_best_grade_and_game_days_count_sessions() {
        let today = Local::now().date_naive();
        let yesterday = today.pred_opt().expect("yesterday");
        let records = [
            record(Movement::ShoulderAbduction, 2, Some(at(today, 10))),
            record(Movement::ShoulderAbduction, 3, Some(at(today, 20))),
            record(Movement::ShoulderAbduction, 1, Some(at(today, 12))),
            record(Movement::ShoulderAbduction, 2, Some(at(yesterday, 12))),
            record(Movement::ElbowFlexion, 5, Some(at(today, 12))),
            record(Movement::ShoulderAbduction, 4, None),
        ];
        let refs: Vec<&MeasurementRecord> = records.iter().collect();
        let deltoid: Vec<(NaiveDate, u8)> = daily_grades(&refs, Muscle::MiddleDeltoid)
            .into_iter()
            .collect();
        assert_eq!(
            deltoid,
            [(yesterday, 2), (today, 3)],
            "best shoulder grade per day; undated and elbow records are skipped"
        );
        assert_eq!(
            daily_grades(&refs, Muscle::BicepsBrachii).get(&today),
            Some(&5)
        );
        assert!(
            daily_grades(&refs, Muscle::PectoralisMajor).is_empty(),
            "no MMT test grades the pectoralis"
        );

        let games = [
            game(GameLevel::PnfD1, Some(at(today, 10))),
            game(GameLevel::PnfD1, Some(at(today, 12))),
            game(GameLevel::PnfD2, Some(at(yesterday, 12))),
            game(GameLevel::AppleTrace, None),
        ];
        let game_refs: Vec<&GameSession> = games.iter().collect();
        let pecs: Vec<(NaiveDate, u32)> = game_days(&game_refs, Muscle::PectoralisMajor)
            .into_iter()
            .collect();
        assert_eq!(
            pecs,
            [(today, 2)],
            "two D1 sessions today train the pectoralis"
        );
        let supra: Vec<(NaiveDate, u32)> = game_days(&game_refs, Muscle::Supraspinatus)
            .into_iter()
            .collect();
        assert_eq!(
            supra,
            [(yesterday, 1)],
            "D2 trains supraspinatus; undated games are skipped"
        );
        assert!(game_days(&game_refs, Muscle::BicepsBrachii).is_empty());
    }
}
