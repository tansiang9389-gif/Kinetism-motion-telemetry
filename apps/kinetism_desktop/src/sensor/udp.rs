//! Wi-Fi sensor input: `KIN1` UDP packets (single IMU, used as the upper-arm sensor).

use super::frame::SensorFrame;
use super::{LinkState, Shared, Source, kin1, with};
use std::net::{IpAddr, UdpSocket as StdUdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DEFAULT_PORT: u16 = 4210;
const SILENCE: Duration = Duration::from_secs(2);
/// The firmware sends 100 Hz; forwarding ~25 Hz keeps calibration timing and
/// hand smoothing the same as the 20 Hz Bluetooth stream.
const FORWARD_INTERVAL: Duration = Duration::from_millis(40);

/// This PC's LAN address (what the firmware's `UDP_HOST` must be). No packet is sent.
pub fn local_ip() -> Option<IpAddr> {
    let socket = StdUdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

/// True when a frame should be forwarded at `now`.
pub fn due(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|t| now.duration_since(t) >= FORWARD_INTERVAL)
}

pub async fn listen(shared: Arc<Mutex<Shared>>, port: u16, ctx: eframe::egui::Context) {
    let socket = match tokio::net::UdpSocket::bind(("0.0.0.0", port)).await {
        Ok(s) => s,
        Err(e) => {
            with(&shared, |s| {
                s.set_state(
                    Source::Wifi,
                    LinkState::Error(format!(
                        "Cannot listen on UDP port {port} ({e}). Close the PC dashboard (python -m pc.app.main) if it is running."
                    )),
                )
            });
            ctx.request_repaint();
            return;
        }
    };
    let waiting = format!("UDP {port}");
    with(&shared, |s| {
        s.set_state(Source::Wifi, LinkState::Listening(waiting.clone()))
    });
    ctx.request_repaint();
    let mut buf = [0u8; 512];
    let mut sensor: Option<IpAddr> = None;
    let mut last_forward: Option<Instant> = None;
    loop {
        match tokio::time::timeout(SILENCE, socket.recv_from(&mut buf)).await {
            Err(_) => {
                sensor = None;
                with(&shared, |s| {
                    s.set_state(Source::Wifi, LinkState::Listening(waiting.clone()))
                });
                ctx.request_repaint();
            }
            // Transient socket errors (e.g. ICMP resets on Windows): keep listening.
            Ok(Err(_)) => tokio::time::sleep(Duration::from_millis(50)).await,
            Ok(Ok((len, from))) => {
                // Follow one sensor at a time; another board can take over after silence.
                if sensor.is_some_and(|ip| ip != from.ip()) {
                    continue;
                }
                let now = Instant::now();
                with(&shared, |s| match kin1::parse(&buf[..len]) {
                    Ok(packet) => {
                        sensor = Some(from.ip());
                        let forward = due(last_forward, now);
                        if forward {
                            last_forward = Some(now);
                            s.push(
                                Source::Wifi,
                                SensorFrame {
                                    upper: Some(packet.quaternion),
                                    forearm: None,
                                },
                            );
                        }
                        s.note_wifi(&packet, !forward);
                        let label = format!("Wi-Fi sensor {}", from.ip());
                        if !matches!(s.state(), LinkState::Streaming(l) if *l == label) {
                            s.set_state(Source::Wifi, LinkState::Streaming(label));
                        }
                    }
                    Err(e) => s.invalid(Source::Wifi, e.to_string()),
                });
                ctx.request_repaint();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_about_25_frames_per_second_from_100_hz() {
        let start = Instant::now();
        let mut last = None;
        let mut forwarded = 0;
        for i in 0..100u64 {
            let now = start + Duration::from_millis(10 * i);
            if due(last, now) {
                last = Some(now);
                forwarded += 1;
            }
        }
        assert_eq!(forwarded, 25);
    }
}
