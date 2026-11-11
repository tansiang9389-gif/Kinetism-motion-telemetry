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
/// `KIN1` packets are 64 bytes. On Windows a datagram larger than the buffer
/// fails the receive, so leave room for anything else sent to the port.
const RECV_BUFFER: usize = 2048;
const LOCAL_IP_TTL: Duration = Duration::from_secs(10);

/// This PC's LAN address (what the firmware's `UDP_HOST` must be). No packet is sent.
pub fn local_ip() -> Option<IpAddr> {
    let socket = StdUdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

/// [`local_ip`] looked up at most every 10 s: it opens a socket, too costly per frame.
#[derive(Default)]
pub struct LocalIpCache {
    last: Option<(Instant, Option<IpAddr>)>,
}

impl LocalIpCache {
    pub fn get(&mut self) -> Option<IpAddr> {
        self.get_at(Instant::now(), local_ip)
    }

    fn get_at(&mut self, now: Instant, lookup: impl FnOnce() -> Option<IpAddr>) -> Option<IpAddr> {
        match self.last {
            Some((at, ip)) if now.saturating_duration_since(at) < LOCAL_IP_TTL => ip,
            _ => {
                let ip = lookup();
                self.last = Some((now, ip));
                ip
            }
        }
    }
}

/// True when a frame should be forwarded at `now`.
pub fn due(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|t| now.duration_since(t) >= FORWARD_INTERVAL)
}

/// The board the listener follows: the first that sends a valid packet, until
/// it has been silent for [`SILENCE`]. Packets from other boards neither reach
/// the tracker nor keep a silent board followed.
#[derive(Debug, Default)]
struct Follow {
    sensor: Option<(IpAddr, Instant)>,
}

impl Follow {
    /// True while another board is followed.
    fn ignores(&self, from: IpAddr) -> bool {
        self.sensor.is_some_and(|(ip, _)| ip != from)
    }

    /// A valid packet arrived from `from` (not ignored): follow it.
    fn heard(&mut self, from: IpAddr, now: Instant) {
        self.sensor = Some((from, now));
    }

    /// Lets go of a board that has been silent too long; true if it did.
    fn expire(&mut self, now: Instant) -> bool {
        let silent = self
            .sensor
            .is_some_and(|(_, last)| now.saturating_duration_since(last) >= SILENCE);
        if silent {
            self.sensor = None;
        }
        silent
    }
}

fn wait_for_packets(shared: &Arc<Mutex<Shared>>, waiting: &str, ctx: &eframe::egui::Context) {
    with(shared, |s| {
        s.set_state(Source::Wifi, LinkState::Listening(waiting.to_string()))
    });
    ctx.request_repaint();
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
    wait_for_packets(&shared, &waiting, &ctx);
    let mut buf = vec![0u8; RECV_BUFFER];
    let mut follow = Follow::default();
    let mut last_forward: Option<Instant> = None;
    let mut errors = 0u32;
    loop {
        match tokio::time::timeout(SILENCE, socket.recv_from(&mut buf)).await {
            Err(_) => {
                follow = Follow::default();
                wait_for_packets(&shared, &waiting, &ctx);
            }
            // Oversized datagrams and ICMP resets (Windows) only cost one datagram:
            // carry on at once. Only a socket that keeps failing is slowed down.
            Ok(Err(_)) => {
                errors = errors.saturating_add(1);
                if errors < 64 {
                    tokio::task::yield_now().await;
                } else {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
            Ok(Ok((len, from))) => {
                errors = 0;
                let now = Instant::now();
                if follow.expire(now) {
                    wait_for_packets(&shared, &waiting, &ctx);
                }
                if follow.ignores(from.ip()) {
                    continue;
                }
                with(&shared, |s| match kin1::parse(&buf[..len]) {
                    Ok(packet) => {
                        follow.heard(from.ip(), now);
                        s.follow_wifi(from.ip());
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

    #[test]
    fn follows_one_board_until_it_falls_silent_even_while_another_keeps_sending() {
        let first: IpAddr = "192.168.1.20".parse().expect("address");
        let second: IpAddr = "192.168.1.21".parse().expect("address");
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut follow = Follow::default();
        assert!(
            !follow.ignores(first),
            "the first board to send is followed"
        );
        follow.heard(first, at(0));
        follow.heard(first, at(500));
        for ms in (600..2500).step_by(10) {
            assert!(
                !follow.expire(at(ms)),
                "{ms} ms: within 2 s of its last packet"
            );
            assert!(
                follow.ignores(second),
                "{ms} ms: the second board is ignored"
            );
        }
        assert!(
            follow.expire(at(2500)),
            "2 s without its own packets frees it, although the second board kept sending"
        );
        assert!(!follow.expire(at(2510)), "released once");
        assert!(!follow.ignores(second));
        follow.heard(second, at(2510));
        assert!(follow.ignores(first), "the second board took over");
    }

    #[test]
    fn the_local_address_is_looked_up_at_most_every_ten_seconds() {
        let start = Instant::now();
        let at = |s: u64| start + Duration::from_secs(s);
        let lan: IpAddr = "192.168.1.5".parse().expect("address");
        let mut lookups = 0;
        let mut cache = LocalIpCache::default();
        let mut get = |cache: &mut LocalIpCache, t, ip| {
            cache.get_at(t, || {
                lookups += 1;
                ip
            })
        };
        assert_eq!(get(&mut cache, at(0), None), None, "offline");
        assert_eq!(
            get(&mut cache, at(9), Some(lan)),
            None,
            "remembered for 10 s"
        );
        assert_eq!(
            get(&mut cache, at(10), Some(lan)),
            Some(lan),
            "then looked up again"
        );
        assert_eq!(get(&mut cache, at(15), None), Some(lan));
        assert_eq!(lookups, 2, "one socket per 10 s, not one per frame");
    }
}
