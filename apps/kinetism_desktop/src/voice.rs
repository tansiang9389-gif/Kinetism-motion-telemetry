//! Voice encouragement through the platform text-to-speech engine.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

const MIN_GAP: Duration = Duration::from_millis(2200);

pub struct Voice {
    tx: Option<Sender<(String, bool)>>,
    last: Option<Instant>,
    pub enabled: bool,
    pub error: Option<String>,
}

impl Voice {
    pub fn new(enabled: bool) -> Self {
        let (tx, rx) = channel::<(String, bool)>();
        let (ready_tx, ready_rx) = channel::<Result<(), String>>();
        let spawned = std::thread::Builder::new()
            .name("kinetism-voice".into())
            .spawn(move || speaker(rx, ready_tx));
        let error = match spawned {
            Err(e) => Some(e.to_string()),
            Ok(_) => match ready_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(())) => None,
                Ok(Err(e)) => Some(e),
                Err(_) => Some("speech engine did not start".into()),
            },
        };
        Self {
            tx: error.is_none().then_some(tx),
            last: None,
            enabled,
            error,
        }
    }

    /// Speak unless another line was spoken very recently. `urgent` interrupts.
    pub fn say(&mut self, text: &str, urgent: bool) {
        if !self.enabled {
            return;
        }
        let Some(tx) = &self.tx else {
            return;
        };
        if !urgent && self.last.is_some_and(|t| t.elapsed() < MIN_GAP) {
            return;
        }
        self.last = Some(Instant::now());
        let _ = tx.send((text.to_string(), urgent));
    }
}

fn speaker(rx: Receiver<(String, bool)>, ready: Sender<Result<(), String>>) {
    let mut tts = match tts::Tts::default() {
        Ok(t) => {
            let _ = ready.send(Ok(()));
            t
        }
        Err(e) => {
            let _ = ready.send(Err(format!("Text-to-speech unavailable: {e}")));
            return;
        }
    };
    let normal = tts.normal_rate();
    let _ = tts.set_rate(normal * 0.95);
    while let Ok((text, urgent)) = rx.recv() {
        let _ = tts.speak(text, urgent);
    }
}
