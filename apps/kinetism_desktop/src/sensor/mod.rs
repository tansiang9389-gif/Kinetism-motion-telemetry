//! Sensor link: live BLE (`Physio_A`) or the built-in simulator.

pub mod ble;
pub mod frame;

use frame::SensorFrame;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone, Debug, PartialEq)]
pub enum LinkState {
    Idle,
    Scanning,
    Connecting(String),
    Streaming(String),
    Reconnecting(String),
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
            LinkState::Scanning | LinkState::Connecting(_) | LinkState::Reconnecting(_)
        )
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
    pub forearm_present: bool,
    pub last_invalid: Option<String>,
}

pub(crate) struct Shared {
    state: LinkState,
    queue: VecDeque<TimedFrame>,
    arrivals: VecDeque<Instant>,
    stats: LinkStats,
}

const MAX_QUEUE: usize = 256;

impl Shared {
    pub(crate) fn set_state(&mut self, state: LinkState) {
        self.state = state;
    }

    pub(crate) fn push(&mut self, frame: SensorFrame) {
        let now = Instant::now();
        self.stats.packets += 1;
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

    pub(crate) fn invalid(&mut self, reason: String) {
        self.stats.invalid_packets += 1;
        self.stats.last_invalid = Some(reason);
    }
}

pub struct SensorHub {
    shared: Arc<Mutex<Shared>>,
    commands: UnboundedSender<ble::BleCommand>,
}

impl SensorHub {
    pub fn new(runtime: &tokio::runtime::Handle, ctx: eframe::egui::Context) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            state: LinkState::Idle,
            queue: VecDeque::new(),
            arrivals: VecDeque::new(),
            stats: LinkStats::default(),
        }));
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        runtime.spawn(ble::run(shared.clone(), rx, ctx));
        Self {
            shared,
            commands: tx,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn connect(&self) {
        let _ = self.commands.send(ble::BleCommand::Connect);
    }

    pub fn disconnect(&self) {
        let _ = self.commands.send(ble::BleCommand::Disconnect);
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

    pub fn start_simulation(&self) {
        self.disconnect();
        let mut s = self.lock();
        s.state = LinkState::Simulated;
        s.queue.clear();
    }

    pub fn stop_simulation(&self) {
        let mut s = self.lock();
        if s.state == LinkState::Simulated {
            s.state = LinkState::Idle;
        }
    }

    /// Inject a simulated packet through the same parser as BLE data.
    pub fn push_simulated(&self, payload: &str) {
        let mut s = self.lock();
        if s.state != LinkState::Simulated {
            return;
        }
        match frame::parse_frame(payload.as_bytes()) {
            Ok(f) => s.push(f),
            Err(e) => s.invalid(e.to_string()),
        }
    }
}
