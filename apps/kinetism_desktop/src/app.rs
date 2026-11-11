//! Application shell: state, navigation, top/status bars, simulator and demo mode.

use crate::clinical::{Anthropometrics, Movement, Muscle, Side};
use crate::game::fx::Fx;
use crate::game::{Game, GameEvent, Stage};
use crate::model::{GameLevel, GameSession, Patient};
use crate::sensor::{LinkState, SensorHub};
use crate::sim::{SimDriver, SimInput};
use crate::store::{DataStore, firestore};
use crate::theme;
use crate::tracker::ArmTracker;
use crate::ui;
use crate::voice::Voice;
use eframe::egui::{self, Align, Color32, Layout, Margin, RichText};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Patients,
    Hub,
    Measure,
    Play,
    Progress,
    Records,
}

#[derive(Default)]
pub struct PatientsState {
    pub search: String,
    pub creating: bool,
    pub name: String,
    pub code: String,
    pub side: Side,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasureStage {
    Setup,
    Calibrate,
    Live,
}

pub struct MeasureState {
    pub movement: Movement,
    pub stage: MeasureStage,
    pub bicep: String,
    pub forearm: String,
    pub length: String,
    pub error: Option<String>,
    pub last_record: Option<String>,
}

pub struct PlayState {
    pub level: GameLevel,
    pub game: Option<Game>,
    pub fx: Fx,
    pub result: Option<GameSession>,
    pub unlocked: Option<GameLevel>,
    pub paused: bool,
    pub bombs: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audience {
    Patient,
    Physio,
}

pub struct ProgressState {
    pub audience: Audience,
    pub muscle: Muscle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordsTab {
    Measurements,
    Games,
}

pub struct RecordsState {
    pub search: String,
    pub overall: bool,
    pub tab: RecordsTab,
}

pub struct Toast {
    pub text: String,
    pub at: Instant,
    pub error: bool,
}

pub struct Demo {
    pub scenario: String,
    pub screenshot: Option<PathBuf>,
    pub started: Instant,
    pub delay: f64,
    pub requested: bool,
    pub clock: f64,
    pub cal_timer: f64,
}

pub struct KinetismApp {
    _runtime: tokio::runtime::Runtime,
    pub store: DataStore,
    pub sensor: SensorHub,
    pub tracker: ArmTracker,
    pub sim: SimDriver,
    pub voice: Voice,
    pub screen: Screen,
    pub patients: PatientsState,
    pub measure: MeasureState,
    pub play: PlayState,
    pub progress: ProgressState,
    pub records: RecordsState,
    pub toast: Option<Toast>,
    pub demo: Option<Demo>,
    pub project: Option<String>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

impl KinetismApp {
    pub fn new(cc: &eframe::CreationContext<'_>, runtime: tokio::runtime::Runtime) -> Self {
        theme::apply(&cc.egui_ctx);
        let demo = env("KINETISM_DEMO").map(|scenario| Demo {
            scenario,
            screenshot: env("KINETISM_SCREENSHOT").map(PathBuf::from),
            started: Instant::now(),
            delay: env("KINETISM_SCREENSHOT_DELAY")
                .and_then(|v| v.parse().ok())
                .unwrap_or(6.0),
            requested: false,
            clock: 0.0,
            cal_timer: 0.0,
        });
        // Demo data is synthetic: never sync it to the shared Firestore project.
        let project = if demo.is_some() || env("KINETISM_OFFLINE").is_some() {
            None
        } else {
            Some(
                env("KINETISM_FIREBASE_PROJECT")
                    .unwrap_or_else(|| firestore::DEFAULT_PROJECT_ID.into()),
            )
        };
        let data_dir = if demo.is_some() {
            let dir = std::env::temp_dir().join("kinetism-desktop-demo");
            let _ = std::fs::remove_dir_all(&dir);
            dir
        } else {
            crate::store::data_dir()
        };
        let store = DataStore::open(
            runtime.handle(),
            cc.egui_ctx.clone(),
            project.clone(),
            data_dir,
        );
        let sensor = SensorHub::new(runtime.handle(), cc.egui_ctx.clone());
        let side = store.settings.side;
        let voice = Voice::new(store.settings.voice && demo.is_none());
        let mut app = Self {
            _runtime: runtime,
            tracker: ArmTracker::new(side),
            sim: SimDriver::new(side),
            voice,
            screen: Screen::Patients,
            patients: PatientsState::default(),
            measure: MeasureState {
                movement: Movement::ShoulderAbduction,
                stage: MeasureStage::Setup,
                bicep: String::new(),
                forearm: String::new(),
                length: String::new(),
                error: None,
                last_record: None,
            },
            play: PlayState {
                level: GameLevel::AppleTrace,
                game: None,
                fx: Fx::default(),
                result: None,
                unlocked: None,
                paused: false,
                bombs: true,
            },
            progress: ProgressState {
                audience: Audience::Physio,
                muscle: Muscle::MiddleDeltoid,
            },
            records: RecordsState {
                search: String::new(),
                overall: false,
                tab: RecordsTab::Measurements,
            },
            toast: None,
            demo,
            project,
            store,
            sensor,
        };
        if env("KINETISM_SIM").is_some() || app.demo.is_some() {
            app.sensor.start_simulation();
        }
        if let Some(id) = app.store.settings.selected_patient.clone()
            && app.store.patient(&id).is_some()
        {
            app.select_patient(&id, false);
        }
        if app.demo.is_some() {
            crate::demo::setup(&mut app);
        }
        app
    }

    pub fn current_patient(&self) -> Option<Patient> {
        self.store
            .settings
            .selected_patient
            .as_deref()
            .and_then(|id| self.store.patient(id))
            .cloned()
    }

    pub fn side(&self) -> Side {
        self.tracker.side
    }

    pub fn select_patient(&mut self, id: &str, navigate: bool) {
        self.store.settings.selected_patient = Some(id.to_string());
        if let Some(p) = self.store.patient(id).cloned() {
            let side = p.affected_side.unwrap_or(self.store.settings.side);
            self.set_side(side);
            let arm = p.arm.unwrap_or_default();
            self.measure.bicep = format!("{:.1}", arm.bicep_cm);
            self.measure.forearm = format!("{:.1}", arm.forearm_cm);
            self.measure.length = format!("{:.1}", arm.arm_length_cm);
        }
        self.store.save_cache();
        if navigate {
            self.screen = Screen::Hub;
        }
    }

    pub fn set_side(&mut self, side: Side) {
        self.tracker.set_side(side);
        self.sim.set_side(side);
        if self.store.settings.side != side {
            self.store.settings.side = side;
            self.store.save_cache();
        }
    }

    pub fn arm(&self) -> Anthropometrics {
        let parse = |s: &str| s.trim().parse::<f64>().ok();
        match (
            parse(&self.measure.bicep),
            parse(&self.measure.forearm),
            parse(&self.measure.length),
        ) {
            (Some(b), Some(f), Some(l)) => Anthropometrics {
                bicep_cm: b,
                forearm_cm: f,
                arm_length_cm: l,
            },
            _ => self
                .current_patient()
                .and_then(|p| p.arm)
                .unwrap_or_default(),
        }
    }

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            at: Instant::now(),
            error,
        });
    }

    pub fn simulated(&self) -> bool {
        self.sensor.state() == LinkState::Simulated
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
            let p = ui.painter();
            p.circle_filled(rect.center(), 15.0, theme::ACCENT);
            p.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "K",
                egui::FontId::proportional(18.0),
                theme::BG,
            );
            ui.label(RichText::new("Kinetism").size(20.0).strong());
            ui.label(RichText::new("Desktop").size(20.0).color(theme::MUTED));
            ui.add_space(12.0);
            if self.screen != Screen::Patients && ui.button("← Back").clicked() {
                self.back();
            }
            if let Some(p) = self.current_patient()
                && self.screen != Screen::Patients
            {
                ui::chip(ui, &format!("{} · {}", p.name, p.code), theme::ACCENT);
                ui::chip(ui, &format!("{} arm", self.side().label()), theme::MUTED);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let voice_label = if self.voice.enabled {
                    "🔊 Voice"
                } else {
                    "🔇 Voice"
                };
                let voice = ui.selectable_label(self.voice.enabled, voice_label);
                let voice = match &self.voice.error {
                    Some(e) => voice.on_hover_text(format!("Voice unavailable: {e}")),
                    None => voice.on_hover_text("Spoken encouragement (Windows text-to-speech)"),
                };
                if voice.clicked() {
                    self.voice.enabled = !self.voice.enabled;
                    self.store.settings.voice = self.voice.enabled;
                    self.store.save_cache();
                }
                self.sync_chip(ui);
                ui.separator();
                self.sensor_controls(ui);
            });
        });
    }

    fn sync_chip(&mut self, ui: &mut egui::Ui) {
        let status = self.store.status().clone();
        let pending = self.store.pending_count();
        let (text, color) = if !status.enabled {
            ("Local only".to_string(), theme::MUTED)
        } else if let Some(err) = &status.last_error {
            let short: String = err.chars().take(60).collect();
            (format!("⚠ Sync: {short}"), theme::WARN)
        } else if status.loading {
            ("⟳ Syncing…".to_string(), theme::MUTED)
        } else if pending > 0 {
            (format!("⟳ {pending} pending"), theme::WARN)
        } else {
            (
                format!(
                    "☁ Synced {}",
                    status
                        .last_pull
                        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
                        .unwrap_or_default()
                ),
                theme::GOOD,
            )
        };
        let r = ui::chip(ui, &text, color);
        let tip = match &self.project {
            Some(p) => format!(
                "Firestore project {p}: Patients, MeasurementRecords and GameSessions are shared with the phone app. {pending} write(s) waiting.{}",
                status
                    .last_error
                    .map(|e| format!("\nLast error: {e}"))
                    .unwrap_or_default()
            ),
            None => "Firestore sync is off (demo or KINETISM_OFFLINE).".into(),
        };
        if r.on_hover_text(tip).clicked()
            || (status.enabled && ui.small_button("↻").on_hover_text("Sync now").clicked())
        {
            self.store.refresh();
        }
    }

    fn sensor_controls(&mut self, ui: &mut egui::Ui) {
        let state = self.sensor.state();
        let stats = self.sensor.stats();
        if state == LinkState::Simulated {
            if ui.button("Stop simulator").clicked() {
                self.sensor.stop_simulation();
            }
        } else if ui
            .button("Simulator")
            .on_hover_text("Demo without hardware: a virtual arm generates real sensor packets.")
            .clicked()
        {
            self.sensor.start_simulation();
        }
        match &state {
            LinkState::Idle | LinkState::Error(_) => {
                if ui::primary(ui, "Connect Physio_A").clicked() {
                    self.sensor.connect();
                }
            }
            LinkState::Simulated => {}
            _ => {
                if ui.button("Disconnect").clicked() {
                    self.sensor.disconnect();
                }
            }
        }
        let (text, color) = match &state {
            LinkState::Idle => ("Sensor: not connected".to_string(), theme::MUTED),
            LinkState::Scanning => ("Scanning for Physio_A…".to_string(), theme::WARN),
            LinkState::Connecting(n) => (format!("Connecting {n}…"), theme::WARN),
            LinkState::Streaming(n) => (format!("● {n} {:.0} Hz", stats.rate_hz), theme::GOOD),
            LinkState::Reconnecting(m) => (format!("Reconnecting: {m}"), theme::WARN),
            LinkState::Simulated => (
                format!("● Simulator {:.0} Hz", stats.rate_hz),
                theme::ACCENT,
            ),
            LinkState::Error(_) => ("Sensor error".to_string(), theme::ERROR),
        };
        let r = ui::chip(ui, &text, color);
        if let LinkState::Error(e) = &state {
            r.on_hover_text(e);
        }
        if state.is_busy() {
            ui.spinner();
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let stats = self.sensor.stats();
            let cal = if self.tracker.is_calibrated() {
                format!("Calibrated · {} arm", self.side().label())
            } else {
                "Not calibrated".into()
            };
            ui.label(ui::small(cal));
            ui.separator();
            ui.label(ui::small(if stats.packets == 0 {
                "No sensor data yet".to_string()
            } else if stats.forearm_present {
                "Upper arm ✓  Forearm ✓".to_string()
            } else {
                "Upper arm ✓  Forearm sensor missing (shoulder still works)".to_string()
            }));
            ui.separator();
            ui.label(ui::small(format!(
                "{} packets · {} invalid",
                stats.packets, stats.invalid_packets
            )));
            if let LinkState::Error(e) = self.sensor.state() {
                ui.separator();
                ui.label(RichText::new(e).size(13.0).color(theme::ERROR));
            }
            if let Some(err) = self.store.cache_error() {
                ui.separator();
                ui.label(RichText::new(err).size(13.0).color(theme::WARN));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(ui::small(
                    "Prototype · not a clinically validated device · MMT grades are suggestions for physiotherapist review",
                ));
            });
        });
    }

    fn back(&mut self) {
        if self.play.game.is_some() && self.screen == Screen::Play {
            self.play.game = None;
            self.play.result = None;
            return;
        }
        self.screen = match self.screen {
            Screen::Patients | Screen::Hub => Screen::Patients,
            Screen::Measure if self.measure.stage == MeasureStage::Live => {
                self.measure.stage = MeasureStage::Setup;
                Screen::Measure
            }
            _ => Screen::Hub,
        };
    }

    fn simulator_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("Simulator")
            .open(&mut open)
            .default_pos(egui::pos2(24.0, 520.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.label(ui::small(
                    "Virtual arm → quaternion packets → same pipeline as Physio_A.",
                ));
                let mouse = matches!(self.sim.input, SimInput::HandTarget(_));
                if mouse {
                    ui.label(
                        RichText::new("Hand follows the mouse over the game.").color(theme::ACCENT),
                    );
                    if ui.button("Use sliders").clicked() {
                        self.sim.input = SimInput::Sliders;
                    }
                } else {
                    ui.add(
                        egui::Slider::new(&mut self.sim.arm.elevation_deg, 0.0..=180.0)
                            .text("Elevation °"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.sim.arm.plane_deg, -60.0..=120.0)
                            .text("Plane ° (0 side, 90 front)"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.sim.arm.elbow_deg, 0.0..=150.0)
                            .text("Elbow flexion °"),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button("Rest pose").clicked() {
                        self.sim.rest_pose();
                    }
                    if ui.button("Forward pose").clicked() {
                        self.sim.forward_pose();
                    }
                });
            });
        if !open {
            self.sensor.stop_simulation();
        }
    }

    fn toast_overlay(&mut self, ctx: &egui::Context) {
        let Some(toast) = &self.toast else {
            return;
        };
        if toast.at.elapsed().as_secs_f32() > 4.0 {
            self.toast = None;
            return;
        }
        egui::Area::new(egui::Id::unique("toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -48.0))
            .show(ctx, |ui| {
                let color = if toast.error {
                    theme::ERROR
                } else {
                    theme::ACCENT
                };
                ui::tinted_card(ui, color, |ui| {
                    ui.label(RichText::new(&toast.text).color(theme::TEXT));
                });
            });
        ctx.request_repaint();
    }

    /// Advance the running game from the latest hand position.
    fn update_game(&mut self, dt: f64) {
        self.play.fx.update(dt);
        let Some(hand) = self.tracker.hand else {
            return;
        };
        let elevation = self.tracker.pose.map_or(0.0, |p| p.elevation_deg);
        let Some(game) = self.play.game.as_mut() else {
            return;
        };
        if self.play.paused || game.stage == Stage::Finished {
            return;
        }
        if game.stage == Stage::Playing {
            self.play.fx.add_trail(hand);
        }
        let events = game.update(hand, elevation, dt);
        self.handle_game_events(&events);
    }

    pub fn handle_game_events(&mut self, events: &[GameEvent]) {
        for event in events {
            crate::ui::game_view::apply_event(self, *event);
        }
    }
}

impl eframe::App for KinetismApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let dt = f64::from(ctx.input(|i| i.stable_dt).clamp(0.0, 0.1));
        self.store.poll();
        if self.simulated() {
            crate::demo::autopilot(self, dt);
            self.sim.tick(&self.sensor, dt);
        }
        let frames = self.sensor.drain();
        self.tracker.feed(&frames);
        self.update_game(dt);

        egui::Panel::top("top")
            .frame(
                egui::Frame::new()
                    .fill(theme::DEEP)
                    .inner_margin(Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(theme::DEEP)
                    .inner_margin(Margin::symmetric(16, 6)),
            )
            .show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::same(20)),
            )
            .show(ui, |ui| match self.screen {
                Screen::Patients => self.patients_screen(ui),
                Screen::Hub => self.hub_screen(ui),
                Screen::Measure => self.measure_screen(ui),
                Screen::Play => self.play_screen(ui),
                Screen::Progress => self.progress_screen(ui),
                Screen::Records => self.records_screen(ui),
            });
        if self.simulated() && self.demo.is_none() {
            self.simulator_window(&ctx);
        }
        self.toast_overlay(&ctx);
        if self.sensor.state().is_live()
            || self.play.game.is_some()
            || !self.play.fx.particles.is_empty()
        {
            ctx.request_repaint();
        }
        crate::demo::tick(self, &ctx, dt);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::from_rgb(0x10, 0x1B, 0x22).to_normalized_gamma_f32()
    }
}
