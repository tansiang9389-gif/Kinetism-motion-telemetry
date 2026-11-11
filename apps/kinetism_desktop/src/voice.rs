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

#[cfg(test)]
mod tests {
    use super::*;

    /// A voice wired to a channel instead of the speech engine.
    fn listening(enabled: bool) -> (Voice, Receiver<(String, bool)>) {
        let (tx, rx) = channel();
        let voice = Voice {
            tx: Some(tx),
            last: None,
            enabled,
            error: None,
        };
        (voice, rx)
    }

    fn heard(rx: &Receiver<(String, bool)>) -> Vec<(String, bool)> {
        rx.try_iter().collect()
    }

    fn line(text: &str, urgent: bool) -> (String, bool) {
        (text.to_string(), urgent)
    }

    #[test]
    fn lines_closer_than_the_gap_are_dropped_unless_urgent() {
        let (mut voice, rx) = listening(true);
        voice.say("Nice slice!", false);
        voice.say("Combo 2!", false);
        assert_eq!(
            heard(&rx),
            [line("Nice slice!", false)],
            "the second line came too soon"
        );
        voice.say("Watch out, bomb!", true);
        assert_eq!(
            heard(&rx),
            [line("Watch out, bomb!", true)],
            "urgent lines interrupt"
        );
        voice.say("Keep going", false);
        assert!(heard(&rx).is_empty(), "an urgent line restarts the gap");

        let ago = |d: Duration| {
            Instant::now()
                .checked_sub(d)
                .expect("the clock has run for a few seconds")
        };
        voice.last = Some(ago(Duration::from_millis(1500)));
        voice.say("Still too soon", false);
        assert!(heard(&rx).is_empty(), "1.5 s is inside the 2.2 s gap");
        voice.last = Some(ago(MIN_GAP + Duration::from_millis(100)));
        voice.say("Halfway there", false);
        assert_eq!(heard(&rx), [line("Halfway there", false)]);
    }

    #[test]
    fn a_muted_voice_or_missing_speech_engine_stays_silent() {
        let (mut voice, rx) = listening(false);
        voice.say("Recorded. 90 degrees.", true);
        assert!(heard(&rx).is_empty());
        assert!(voice.last.is_none(), "muted lines do not start the gap");
        voice.enabled = true;
        voice.say("Recorded. 90 degrees.", false);
        assert_eq!(
            heard(&rx),
            [line("Recorded. 90 degrees.", false)],
            "unmuting speaks straight away"
        );

        let mut no_engine = Voice {
            tx: None,
            last: None,
            enabled: true,
            error: Some("Text-to-speech unavailable".into()),
        };
        no_engine.say("Recorded. 90 degrees.", true);
        assert!(no_engine.last.is_none(), "nothing was sent");
    }
}
