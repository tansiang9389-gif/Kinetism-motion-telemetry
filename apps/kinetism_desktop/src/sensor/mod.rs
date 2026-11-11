//! Sensor link: Bluetooth (`Physio_A` pair), Wi-Fi (`KIN1` UDP) or the built-in simulator.

pub mod ble;
pub mod frame;
pub mod kin1;
pub mod udp;

use frame::SensorFrame;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

/// Which input feeds the tracker. Only the active source may change the link state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    None,
    Bluetooth,
    Wifi,
    Simulator,
}

impl Source {
    pub fn from_key(key: &str) -> Option<Source> {
        match key.trim().to_ascii_lowercase().as_str() {
            "bluetooth" | "ble" => Some(Source::Bluetooth),
            "wifi" | "wi-fi" | "udp" => Some(Source::Wifi),
            "sim" | "simulator" => Some(Source::Simulator),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LinkState {
    Idle,
    Scanning,
    Connecting(String),
    Streaming(String),
    /// Connected, but the sensor sends no readings.
    NoData(String),
    Reconnecting(String),
    /// Wi-Fi socket open, waiting for packets.
    Listening(String),
    Simulated,
    Error(String),
}

impl LinkState {
    pub fn is_live(&self) -> bool {
        matches!(self, LinkState::Streaming(_) | LinkState::Simulated)
    }

    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            LinkState::Scanning
                | LinkState::Connecting(_)
                | LinkState::Reconnecting(_)
                | LinkState::Listening(_)
        )
    }

    pub fn is_idle(&self) -> bool {
        matches!(self, LinkState::Idle | LinkState::Error(_))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TimedFrame {
    pub frame: SensorFrame,
    pub at: Instant,
}

#[derive(Clone, Debug, Default)]
pub struct LinkStats {
    pub packets: u64,
    pub invalid_packets: u64,
    pub rate_hz: f32,
    pub upper_present: bool,
    pub forearm_present: bool,
    pub last_invalid: Option<String>,
    /// Reported by the Wi-Fi sensor.
    pub battery_percent: Option<u8>,
    pub accuracy: Option<u8>,
    pub device_rate_hz: Option<f32>,
}

pub(crate) struct Shared {
    source: Source,
    state: LinkState,
    queue: VecDeque<TimedFrame>,
    arrivals: VecDeque<Instant>,
    stats: LinkStats,
}

const MAX_QUEUE: usize = 256;

pub(crate) fn with<R>(shared: &Arc<Mutex<Shared>>, f: impl FnOnce(&mut Shared) -> R) -> R {
    let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

impl Shared {
    fn new() -> Self {
        Self {
            source: Source::None,
            state: LinkState::Idle,
            queue: VecDeque::new(),
            arrivals: VecDeque::new(),
            stats: LinkStats::default(),
        }
    }

    fn reset(&mut self, source: Source, state: LinkState) {
        self.source = source;
        self.state = state;
        self.queue.clear();
        self.arrivals.clear();
        self.stats = LinkStats::default();
    }

    pub(crate) fn state(&self) -> &LinkState {
        &self.state
    }

    /// Ignored unless `from` is the active source (a stopping task cannot overwrite it).
    pub(crate) fn set_state(&mut self, from: Source, state: LinkState) {
        if self.source == from {
            self.state = state;
        }
    }

    pub(crate) fn push(&mut self, from: Source, frame: SensorFrame) {
        if self.source != from {
            return;
        }
        let now = Instant::now();
        self.stats.packets += 1;
        self.stats.upper_present = frame.upper.is_some();
        self.stats.forearm_present = frame.forearm.is_some();
        self.arrivals.push_back(now);
        while self
            .arrivals
            .front()
            .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(2))
        {
            self.arrivals.pop_front();
        }
        self.stats.rate_hz = self.arrivals.len() as f32 / 2.0;
        if self.queue.len() >= MAX_QUEUE {
            self.queue.pop_front();
        }
        self.queue.push_back(TimedFrame { frame, at: now });
    }

    pub(crate) fn invalid(&mut self, from: Source, reason: String) {
        if self.source == from {
            self.stats.invalid_packets += 1;
            self.stats.last_invalid = Some(reason);
        }
    }

    /// Wi-Fi status fields; `skipped` counts a valid packet that was not forwarded.
    pub(crate) fn note_wifi(&mut self, packet: &kin1::Kin1Packet, skipped: bool) {
        if self.source != Source::Wifi {
            return;
        }
        if skipped {
            self.stats.packets += 1;
        }
        self.stats.battery_percent = Some(packet.battery_percent);
        self.stats.accuracy = Some(packet.accuracy);
        self.stats.device_rate_hz = Some(packet.sample_rate_hz);
    }
}

pub struct SensorHub {
    shared: Arc<Mutex<Shared>>,
    ble: UnboundedSender<ble::BleCommand>,
    runtime: tokio::runtime::Handle,
    ctx: eframe::egui::Context,
    wifi_task: Mutex<Option<tokio::task::AbortHandle>>,
    pub udp_port: u16,
}

impl SensorHub {
    pub fn new(
        runtime: &tokio::runtime::Handle,
        ctx: eframe::egui::Context,
        udp_port: u16,
    ) -> Self {
        let shared = Arc::new(Mutex::new(Shared::new()));
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        runtime.spawn(ble::run(shared.clone(), rx, ctx.clone()));
        Self {
            shared,
            ble: tx,
            runtime: runtime.clone(),
            ctx,
            wifi_task: Mutex::new(None),
            udp_port,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Stop every source (the Wi-Fi socket closes, Bluetooth disconnects).
    fn stop_all(&self, next: Source, state: LinkState) {
        let _ = self.ble.send(ble::BleCommand::Disconnect);
        if let Some(task) = self
            .wifi_task
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            task.abort();
        }
        self.lock().reset(next, state);
    }

    pub fn connect_bluetooth(&self) {
        self.stop_all(Source::Bluetooth, LinkState::Scanning);
        let _ = self.ble.send(ble::BleCommand::Connect);
    }

    pub fn connect_wifi(&self) {
        self.stop_all(
            Source::Wifi,
            LinkState::Listening(format!("UDP {}", self.udp_port)),
        );
        let task = self.runtime.spawn(udp::listen(
            self.shared.clone(),
            self.udp_port,
            self.ctx.clone(),
        ));
        *self.wifi_task.lock().unwrap_or_else(|e| e.into_inner()) = Some(task.abort_handle());
    }

    pub fn start_simulation(&self) {
        self.stop_all(Source::Simulator, LinkState::Simulated);
    }

    pub fn disconnect(&self) {
        self.stop_all(Source::None, LinkState::Idle);
    }

    pub fn source(&self) -> Source {
        self.lock().source
    }

    pub fn state(&self) -> LinkState {
        self.lock().state.clone()
    }

    pub fn stats(&self) -> LinkStats {
        self.lock().stats.clone()
    }

    /// Frames received since the previous call.
    pub fn drain(&self) -> Vec<TimedFrame> {
        self.lock().queue.drain(..).collect()
    }

    /// Inject a simulated packet through the same parser as Bluetooth data.
    pub fn push_simulated(&self, payload: &str) {
        let mut s = self.lock();
        match frame::parse_frame(payload.as_bytes()) {
            Ok(f) => s.push(Source::Simulator, f),
            Err(e) => s.invalid(Source::Simulator, e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Quat;

    fn frame() -> SensorFrame {
        SensorFrame {
            upper: Some(Quat::IDENTITY),
            forearm: None,
        }
    }

    #[test]
    fn only_the_active_source_changes_state_and_feeds_frames() {
        let mut s = Shared::new();
        s.reset(Source::Wifi, LinkState::Listening("UDP 4210".into()));
        s.set_state(Source::Bluetooth, LinkState::Idle);
        s.push(Source::Bluetooth, frame());
        s.invalid(Source::Bluetooth, "late packet".into());
        assert_eq!(s.state(), &LinkState::Listening("UDP 4210".into()));
        assert!(s.queue.is_empty());
        assert_eq!(s.stats.invalid_packets, 0);
        s.push(Source::Wifi, frame());
        assert_eq!(s.queue.len(), 1);
        s.reset(Source::Simulator, LinkState::Simulated);
        assert_eq!(s.stats.packets, 0, "stats restart for a new source");
        assert!(s.queue.is_empty());
    }

    #[test]
    fn a_task_that_panics_holding_the_lock_does_not_take_the_link_down() {
        let shared = Arc::new(Mutex::new(Shared::new()));
        let task = shared.clone();
        let crashed = std::thread::spawn(move || {
            with(&task, |s| {
                s.reset(Source::Wifi, LinkState::Listening("UDP 4210".into()));
                panic!("simulated sensor task crash while holding the lock");
            })
        })
        .join();
        assert!(crashed.is_err(), "the sensor task panicked");
        assert!(shared.is_poisoned(), "the panic poisoned the mutex");
        assert_eq!(
            with(&shared, |s| s.state().clone()),
            LinkState::Listening("UDP 4210".into()),
            "the state written before the panic is still readable"
        );
        with(&shared, |s| s.push(Source::Wifi, frame()));
        assert_eq!(
            with(&shared, |s| (s.queue.len(), s.stats.packets)),
            (1, 1),
            "frames keep flowing through the poisoned lock"
        );
    }

    #[test]
    fn parses_source_names() {
        assert_eq!(Source::from_key("WiFi"), Some(Source::Wifi));
        assert_eq!(Source::from_key("ble"), Some(Source::Bluetooth));
        assert_eq!(Source::from_key("sim"), Some(Source::Simulator));
        assert_eq!(Source::from_key("usb"), None);
    }

    #[test]
    fn reports_which_segments_are_streaming() {
        let mut s = Shared::new();
        s.reset(Source::Bluetooth, LinkState::Streaming("Physio_A".into()));
        // Master without a working IMU relaying a working slave.
        s.push(
            Source::Bluetooth,
            SensorFrame {
                upper: None,
                forearm: Some(Quat::IDENTITY),
            },
        );
        assert!(!s.stats.upper_present);
        assert!(s.stats.forearm_present);
        s.push(Source::Bluetooth, frame());
        assert!(s.stats.upper_present);
        assert!(!s.stats.forearm_present);
    }

    #[test]
    fn link_states_choose_connect_menu_spinner_and_repaint() {
        let name = || "Physio_A".to_string();
        // (state, live: repaint + measure, busy: spinner, idle: show "Connect sensor")
        for (state, live, busy, idle) in [
            (LinkState::Idle, false, false, true),
            (
                LinkState::Error("No Bluetooth adapter".into()),
                false,
                false,
                true,
            ),
            (LinkState::Scanning, false, true, false),
            (LinkState::Connecting(name()), false, true, false),
            (LinkState::Reconnecting(name()), false, true, false),
            (LinkState::Listening("UDP 4210".into()), false, true, false),
            (LinkState::Streaming(name()), true, false, false),
            (LinkState::Simulated, true, false, false),
            (LinkState::NoData(name()), false, false, false),
        ] {
            assert_eq!(
                (state.is_live(), state.is_busy(), state.is_idle()),
                (live, busy, idle),
                "{state:?}"
            );
        }
    }

    #[test]
    fn active_source_tracks_invalid_packets_wifi_status_and_bounded_queue() {
        let mut s = Shared::new();
        s.reset(Source::Bluetooth, LinkState::Streaming("Physio_A".into()));
        s.invalid(Source::Bluetooth, "expected 8 values, got 3".into());
        assert_eq!(s.stats.invalid_packets, 1);
        assert_eq!(
            s.stats.last_invalid.as_deref(),
            Some("expected 8 values, got 3")
        );

        let packet = kin1::Kin1Packet {
            flags: kin1::FLAG_SENSOR_OK,
            sequence: 1,
            timestamp_ms: 0,
            quaternion: Quat::IDENTITY,
            battery_percent: 76,
            accuracy: 2,
            sample_rate_hz: 100.0,
        };
        s.note_wifi(&packet, true);
        assert_eq!(
            (s.stats.packets, s.stats.battery_percent),
            (0, None),
            "Wi-Fi status is ignored while Bluetooth is active"
        );

        s.reset(Source::Wifi, LinkState::Listening("UDP 4210".into()));
        s.push(Source::Wifi, frame());
        s.note_wifi(&packet, false);
        s.note_wifi(&packet, true);
        assert_eq!(
            s.stats.packets, 2,
            "forwarded and throttled packets are each counted once"
        );
        assert_eq!(s.queue.len(), 1, "throttled packets are not queued");
        assert_eq!(
            (
                s.stats.battery_percent,
                s.stats.accuracy,
                s.stats.device_rate_hz
            ),
            (Some(76), Some(2), Some(100.0))
        );

        for _ in 0..MAX_QUEUE + 44 {
            s.push(Source::Wifi, frame());
        }
        assert_eq!(
            s.queue.len(),
            MAX_QUEUE,
            "an undrained queue keeps only the newest frames"
        );
        assert_eq!(s.stats.packets, 2 + MAX_QUEUE as u64 + 44);
        assert!(s.stats.rate_hz > 100.0, "rate {}", s.stats.rate_hz);
    }
}
