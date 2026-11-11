//! Application shell: state, navigation, top/status bars, simulator and demo mode.

use crate::clinical::{Anthropometrics, Movement, Muscle, Side};
use crate::game::fx::Fx;
use crate::game::{Game, GameEvent, Stage};
use crate::model::{GameLevel, GameSession, Patient};
use crate::sensor::udp::LocalIpCache;
use crate::sensor::{LinkState, SensorHub, Source};
use crate::sim::{SimDriver, SimInput};
use crate::store::{DataStore, cache_file_name, firestore};
use crate::theme;
use crate::tracker::ArmTracker;
use crate::ui;
use crate::voice::Voice;
use eframe::egui::{self, Align, Color32, Layout, Margin, RichText};
use std::net::IpAddr;
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
    /// Wi-Fi board seen last; another one means a new calibration.
    wifi_sender: Option<IpAddr>,
    local_ip: LocalIpCache,
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
            data_dir.join(cache_file_name(project.as_deref())),
        );
        let udp_port = env("KINETISM_UDP_PORT")
            .and_then(|p| p.parse().ok())
            .unwrap_or(crate::sensor::udp::DEFAULT_PORT);
        let sensor = SensorHub::new(runtime.handle(), cc.egui_ctx.clone(), udp_port);
        let voice = Voice::new(store.settings.voice && demo.is_none());
        let mut app = Self::assemble(runtime, store, sensor, voice, demo, project);
        let start_source = if app.demo.is_some() || env("KINETISM_SIM").is_some() {
            Some(Source::Simulator)
        } else {
            env("KINETISM_SOURCE").and_then(|s| Source::from_key(&s))
        };
        if let Some(source) = start_source {
            app.switch_source(source);
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

    /// The app in its start state (no patient open, nothing connected).
    fn assemble(
        runtime: tokio::runtime::Runtime,
        store: DataStore,
        sensor: SensorHub,
        voice: Voice,
        demo: Option<Demo>,
        project: Option<String>,
    ) -> Self {
        let side = store.settings.side;
        Self {
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
            wifi_sender: None,
            local_ip: LocalIpCache::default(),
        }
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
        if self.store.settings.selected_patient.as_deref() != Some(id) {
            self.forget_previous_patient();
        }
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

    /// Another patient wears re-strapped sensors: their calibration, last
    /// attempt and game must never carry over to the next patient.
    fn forget_previous_patient(&mut self) {
        self.tracker.recalibrate();
        self.measure.stage = MeasureStage::Setup;
        self.measure.error = None;
        self.measure.last_record = None;
        self.play.game = None;
        self.play.result = None;
        self.play.unlocked = None;
        self.play.paused = false;
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
        self.sensor.source() == Source::Simulator
    }

    /// Simulator data is never saved as patient data, except in demo mode,
    /// whose throwaway store never syncs.
    pub fn practice_only(&self) -> bool {
        self.simulated() && self.demo.is_none()
    }

    /// Change the sensor input. A different sensor (or mounting) needs a new calibration.
    pub fn switch_source(&mut self, source: Source) {
        match source {
            Source::Bluetooth => self.sensor.connect_bluetooth(),
            Source::Wifi => self.sensor.connect_wifi(),
            Source::Simulator => self.sensor.start_simulation(),
            Source::None => self.sensor.disconnect(),
        }
        self.wifi_sender = None;
        if source != Source::None {
            self.tracker.recalibrate();
        }
    }

    /// A different Wi-Fi board took over: its mounting and heading are unknown.
    fn note_wifi_sender(&mut self, sender: Option<IpAddr>) {
        let Some(ip) = sender else {
            return;
        };
        if self.wifi_sender.replace(ip).is_some_and(|old| old != ip) {
            self.tracker.recalibrate();
            self.toast(format!("New Wi-Fi sensor {ip}: please recalibrate."), true);
        }
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
        let source = self.sensor.source();
        let stats = self.sensor.stats();
        let port = self.sensor.udp_port;
        let mut choice: Option<Source> = None;
        if source == Source::None || state.is_idle() {
            let button =
                egui::Button::new(RichText::new("Connect sensor").strong().color(theme::BG))
                    .fill(theme::ACCENT)
                    .min_size(egui::vec2(0.0, 40.0));
            egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
                ui.set_min_width(300.0);
                if ui
                    .button("Bluetooth · Physio_A pair (upper arm + forearm)")
                    .clicked()
                {
                    choice = Some(Source::Bluetooth);
                    ui.close();
                }
                if ui
                    .button(format!("Wi-Fi · KIN1 sensor, UDP {port} (single IMU)"))
                    .clicked()
                {
                    choice = Some(Source::Wifi);
                    ui.close();
                }
                ui.separator();
                if ui
                    .button("Simulator (no hardware)")
                    .on_hover_text("A virtual arm generates real sensor packets.")
                    .clicked()
                {
                    choice = Some(Source::Simulator);
                    ui.close();
                }
            });
        } else {
            let label = if source == Source::Simulator {
                "Stop simulator"
            } else {
                "Disconnect"
            };
            if ui.button(label).clicked() {
                choice = Some(Source::None);
            }
        }
        let (text, color) = match &state {
            LinkState::Idle => ("Sensor: not connected".to_string(), theme::MUTED),
            LinkState::Scanning => ("Scanning for Physio_A…".to_string(), theme::WARN),
            LinkState::Connecting(n) => (format!("Connecting {n}…"), theme::WARN),
            LinkState::Streaming(n) => match stats.device_rate_hz {
                Some(rate) => (format!("● {n} · {rate:.0} Hz"), theme::GOOD),
                None => (format!("● {n} {:.0} Hz", stats.rate_hz), theme::GOOD),
            },
            LinkState::NoData(n) => (format!("{n} connected · no readings"), theme::WARN),
            LinkState::Reconnecting(m) => (format!("Reconnecting: {m}"), theme::WARN),
            LinkState::Listening(m) => (format!("Wi-Fi: waiting for packets ({m})"), theme::WARN),
            LinkState::Simulated => (
                format!("● Simulator {:.0} Hz", stats.rate_hz),
                theme::ACCENT,
            ),
            LinkState::Error(_) => ("Sensor error".to_string(), theme::ERROR),
        };
        let r = ui::chip(ui, &text, color);
        match &state {
            LinkState::Error(e) => {
                r.on_hover_text(e);
            }
            LinkState::NoData(n) => {
                r.on_hover_text(format!(
                    "{n} accepted the connection but sends no readings: its IMU is not reporting and no forearm data reaches it. \
                     Switch the sensor off and on. If it persists, check the master's serial log for 'BNO1: FAILED' and that the slave LED is solid (synced)."
                ));
            }
            LinkState::Listening(_) => {
                let ip = self
                    .local_ip
                    .get()
                    .map_or("this PC's IP".to_string(), |ip| ip.to_string());
                r.on_hover_text(format!(
                    "No KIN1 packets yet. Check that the Wi-Fi sensor is switched on and joined the same network, \
                     that its UDP_HOST is {ip} (port {port}), and that Windows Firewall allows Kinetism Desktop."
                ));
            }
            _ => {}
        }
        if state.is_busy() {
            ui.spinner();
        }
        if let Some(source) = choice {
            self.switch_source(source);
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
            let wifi = self.sensor.source() == Source::Wifi;
            let sensors = if stats.packets == 0 {
                "No sensor data yet".to_string()
            } else if wifi {
                format!(
                    "Wi-Fi IMU (upper arm){}{}",
                    stats
                        .battery_percent
                        .map_or(String::new(), |b| format!(" · battery {b}%")),
                    stats
                        .accuracy
                        .map_or(String::new(), |a| format!(" · accuracy {a}/3"))
                )
            } else if !stats.upper_present {
                "Upper-arm IMU (master) not reporting · Forearm ✓ · calibration needs the upper-arm sensor"
                    .to_string()
            } else if stats.forearm_present {
                "Upper arm ✓  Forearm ✓".to_string()
            } else {
                "Upper arm ✓  Forearm sensor missing (shoulder still works)".to_string()
            };
            let sensors = ui.label(ui::small(sensors));
            if wifi {
                sensors.on_hover_text(
                    "One IMU: shoulder abduction and the game work. Elbow flexion needs the two-sensor Bluetooth pair.",
                );
            }
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
            // Right-aligned disclaimer, shortened (full text on hover) when space is tight.
            const FULL: &str = "Prototype · not a clinically validated device · MMT grades are suggestions for physiotherapist review";
            const SHORT: &str = "Prototype · not clinically validated";
            let fits = |ui: &egui::Ui, text: &str| {
                let width = ui
                    .painter()
                    .layout_no_wrap(text.to_string(), egui::FontId::proportional(13.0), theme::MUTED)
                    .size()
                    .x;
                width + 24.0 <= ui.available_width()
            };
            let disclaimer = if fits(ui, FULL) {
                Some(FULL)
            } else if fits(ui, SHORT) {
                Some(SHORT)
            } else {
                None
            };
            if let Some(text) = disclaimer {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(ui::small(text)).on_hover_text(FULL);
                });
            }
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
            self.switch_source(Source::None);
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

    /// Advance the running game from the latest hand position. Without live
    /// upper-arm data the game holds still, so a sensor dropout cannot time out
    /// reps and save a low score.
    fn update_game(&mut self, dt: f64) {
        self.play.fx.update(dt);
        let following = !self.play.paused && self.tracker.has_live_data();
        let hand = self.tracker.hand.filter(|_| following);
        let elevation = self.tracker.pose.map_or(0.0, |p| p.elevation_deg);
        let Some(game) = self.play.game.as_mut() else {
            return;
        };
        if game.stage == Stage::Finished {
            return;
        }
        let Some(hand) = hand else {
            // The hand may move while it is not followed: no slice across the gap.
            game.interrupt();
            return;
        };
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
        // After draining, so frames from a new board are never fed to the old calibration.
        self.note_wifi_sender(self.sensor.stats().wifi_sender);
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::game::{APPLES_PER_ROUND, Mode, is_unlocked};
    use crate::kinematics::{SAMPLES_PER_POSE, SimArm};
    use crate::sensor::TimedFrame;
    use crate::sensor::frame::SensorFrame;
    use std::path::Path;

    /// Scratch folder under the git-ignored `target` directory.
    pub(crate) fn test_dir(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-data")
            .join(crate::model::new_id(name))
    }

    /// The app without a window: a local-only store in `dir`, sensor tasks on
    /// a runtime that is never driven (no radio, no socket) and no speech.
    pub(crate) fn test_app(dir: &Path) -> KinetismApp {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime");
        let ctx = egui::Context::default();
        let store = DataStore::open(
            runtime.handle(),
            ctx.clone(),
            None,
            dir.join(cache_file_name(None)),
        );
        let sensor = SensorHub::new(runtime.handle(), ctx, 0);
        KinetismApp::assemble(runtime, store, sensor, Voice::muted(), None, None)
    }

    pub(crate) fn demo(scenario: &str) -> Demo {
        Demo {
            scenario: scenario.into(),
            screenshot: None,
            started: Instant::now(),
            delay: 6.0,
            requested: false,
            clock: 0.0,
            cal_timer: 0.0,
        }
    }

    fn patient(id: &str) -> Patient {
        Patient {
            id: id.into(),
            name: format!("Patient {id}"),
            code: id.to_uppercase(),
            arm: None,
            affected_side: Some(Side::Right),
            updated_at: None,
        }
    }

    /// Rest-only calibration from simulated sensor frames.
    fn calibrate(tracker: &mut ArmTracker) {
        let (upper, forearm) = SimArm::new(tracker.side).quats();
        let frames: Vec<TimedFrame> = (0..SAMPLES_PER_POSE)
            .map(|_| TimedFrame {
                frame: SensorFrame {
                    upper: Some(upper),
                    forearm: Some(forearm),
                },
                at: Instant::now(),
            })
            .collect();
        tracker.start_rest_capture();
        tracker.feed(&frames);
        tracker.skip_forward_pose();
        assert!(tracker.is_calibrated());
    }

    #[test]
    fn opening_another_patient_starts_over_but_reopening_the_same_one_does_not() {
        let dir = test_dir("switch");
        let mut app = test_app(&dir);
        for id in ["p1", "p2"] {
            app.store.upsert_patient(patient(id));
        }
        app.select_patient("p1", true);
        calibrate(&mut app.tracker);
        app.measure.stage = MeasureStage::Live;
        app.measure.last_record = Some("rec-p1".into());
        app.play.game = Some(Game::new(GameLevel::PnfD1, Side::Right, None, 8, true, 1));
        app.play.paused = true;
        app.play.unlocked = Some(GameLevel::PnfD2);

        app.select_patient("p1", true);
        assert!(app.tracker.is_calibrated(), "same patient, same straps");
        assert_eq!(
            (app.measure.stage, app.measure.last_record.as_deref()),
            (MeasureStage::Live, Some("rec-p1"))
        );
        assert!(app.play.game.is_some());

        app.select_patient("p2", true);
        assert!(
            !app.tracker.is_calibrated(),
            "both patients have the right arm affected, yet the straps were redone"
        );
        assert_eq!(
            app.measure.stage,
            MeasureStage::Setup,
            "Continue leads to calibration, not straight to Live"
        );
        assert!(
            app.measure.last_record.is_none(),
            "patient 1's attempt card and grade buttons are gone"
        );
        assert!(app.play.game.is_none() && app.play.result.is_none());
        assert!(app.play.unlocked.is_none() && !app.play.paused);
        assert_eq!(app.screen, Screen::Hub);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Ends a Level 0 round at 95 %, as the game loop would.
    fn finish_round(app: &mut KinetismApp) {
        app.start_game(GameLevel::AppleTrace);
        let game = app.play.game.as_mut().expect("a running game");
        if let Mode::Apple(round) = &mut game.mode {
            round.scores = vec![0.95; APPLES_PER_ROUND as usize];
            round.done = APPLES_PER_ROUND;
        }
        game.stage = Stage::Finished;
        let finished = GameEvent::Finished {
            score: game.final_score(),
            stars: game.stars(),
        };
        app.handle_game_events(&[finished]);
    }

    #[test]
    fn a_simulator_round_is_shown_but_never_saved_or_unlocking() {
        let dir = test_dir("practice-game");
        let mut app = test_app(&dir);
        app.store.upsert_patient(patient("p1"));
        app.select_patient("p1", false);

        app.switch_source(Source::Simulator);
        finish_round(&mut app);
        let shown = app.play.result.clone().expect("the result is shown");
        assert_eq!(shown.score_percent, 95.0);
        assert!(app.store.games.is_empty(), "not saved or synced");
        assert!(app.play.unlocked.is_none());
        assert!(!is_unlocked(
            &app.store.games_for("p1"),
            GameLevel::PnfD1,
            false
        ));

        app.demo = Some(demo("result"));
        finish_round(&mut app);
        assert_eq!(
            app.store.games.len(),
            1,
            "demo mode keeps saving to its throwaway store"
        );
        assert_eq!(app.play.unlocked, Some(GameLevel::PnfD1));

        app.demo = None;
        app.switch_source(Source::None);
        finish_round(&mut app);
        assert_eq!(
            app.store.games.len(),
            2,
            "rounds played with a sensor are saved"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_different_wifi_board_needs_a_new_calibration() {
        let dir = test_dir("wifi");
        let mut app = test_app(&dir);
        let first: IpAddr = "192.168.1.20".parse().expect("address");
        let second: IpAddr = "192.168.1.21".parse().expect("address");
        calibrate(&mut app.tracker);
        for sender in [Some(first), None, Some(first)] {
            app.note_wifi_sender(sender);
        }
        assert!(
            app.tracker.is_calibrated() && app.toast.is_none(),
            "the same board, back after a gap, keeps its calibration"
        );

        app.note_wifi_sender(Some(second));
        assert!(
            !app.tracker.is_calibrated(),
            "another board's mounting is unknown"
        );
        let toast = app.toast.take().expect("the physio is told");
        assert!(
            toast.error && toast.text.contains("192.168.1.21"),
            "{}",
            toast.text
        );

        app.switch_source(Source::Wifi);
        calibrate(&mut app.tracker);
        app.note_wifi_sender(Some(first));
        assert!(
            app.tracker.is_calibrated() && app.toast.is_none(),
            "the first board after reconnecting is not a change"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
