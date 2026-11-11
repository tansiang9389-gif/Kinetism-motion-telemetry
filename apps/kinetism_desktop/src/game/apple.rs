//! Level 0: trace an apple outline with the hand.

use super::{Pt, dist};
use std::f64::consts::{PI, TAU};

pub const CHECKPOINTS: usize = 96;
const LOOKAHEAD: usize = 6;

#[derive(Clone, Debug)]
pub struct ApplePath {
    pub points: Vec<Pt>,
    pub center: Pt,
    pub radius: f64,
}

/// Apple-shaped closed outline, starting just right of the stem notch and
/// running clockwise (on screen, y up).
pub fn apple_path(center: Pt, radius: f64) -> ApplePath {
    let wrap = |a: f64| {
        let mut r = a % TAU;
        if r > PI {
            r -= TAU;
        }
        if r < -PI {
            r += TAU;
        }
        r
    };
    let start = 0.32;
    let points = (0..CHECKPOINTS)
        .map(|i| {
            let theta = start + (TAU - 2.0 * start) * i as f64 / (CHECKPOINTS - 1) as f64;
            let notch = 1.0 - 0.24 * (-(wrap(theta) / 0.42).powi(2)).exp();
            let dimple = 1.0 - 0.07 * (-(wrap(theta - PI) / 0.45).powi(2)).exp();
            let bulge = 1.0 + 0.05 * theta.sin().powi(2);
            let r = radius * notch * dimple * bulge;
            [
                center[0] + 1.06 * r * theta.sin(),
                center[1] + r * theta.cos(),
            ]
        })
        .collect();
    ApplePath {
        points,
        center,
        radius,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TraceEvent {
    Started,
    Milestone(u8),
    Completed,
}

#[derive(Clone, Debug)]
pub struct AppleTracer {
    pub path: ApplePath,
    pub next: usize,
    pub started: bool,
    pub done: bool,
    pub tolerance: f64,
    pub elapsed: f64,
    on_path: f64,
    traced_time: f64,
    milestone: u8,
}

impl AppleTracer {
    pub fn new(path: ApplePath) -> Self {
        let tolerance = (path.radius * 0.32).clamp(0.055, 0.095);
        Self {
            path,
            next: 0,
            started: false,
            done: false,
            tolerance,
            elapsed: 0.0,
            on_path: 0.0,
            traced_time: 0.0,
            milestone: 0,
        }
    }

    pub fn start_point(&self) -> Pt {
        self.path.points[0]
    }

    pub fn coverage(&self) -> f64 {
        self.next as f64 / self.path.points.len() as f64
    }

    /// Share of tracing time spent on the outline.
    pub fn accuracy(&self) -> f64 {
        if self.traced_time <= 0.0 {
            if self.done { 1.0 } else { 0.0 }
        } else {
            (self.on_path / self.traced_time).clamp(0.0, 1.0)
        }
    }

    /// 0..1 score: coverage, accuracy and a gentle time component.
    pub fn score(&self) -> f64 {
        let time = if self.elapsed <= 30.0 {
            1.0
        } else {
            (1.0 - (self.elapsed - 30.0) / 120.0).clamp(0.5, 1.0)
        };
        0.55 * self.coverage() + 0.30 * self.accuracy() + 0.15 * time * self.coverage()
    }

    pub fn update(&mut self, hand: Pt, dt: f64) -> Vec<TraceEvent> {
        let mut events = Vec::new();
        if self.done {
            return events;
        }
        if !self.started {
            if dist(hand, self.start_point()) <= self.tolerance * 1.6 {
                self.started = true;
                self.next = 1;
                events.push(TraceEvent::Started);
            }
            return events;
        }
        self.elapsed += dt;
        self.traced_time += dt;
        let n = self.path.points.len();
        let window_start = self.next.saturating_sub(3);
        let window_end = (self.next + 3).min(n - 1);
        let nearest = (window_start..=window_end)
            .map(|i| dist(hand, self.path.points[i]))
            .fold(f64::INFINITY, f64::min);
        if nearest <= self.tolerance {
            self.on_path += dt;
        }
        let last = (self.next + LOOKAHEAD).min(n - 1);
        if let Some(reached) = (self.next..=last)
            .rev()
            .find(|&i| dist(hand, self.path.points[i]) <= self.tolerance)
        {
            self.next = reached + 1;
        }
        let percent = (self.coverage() * 100.0) as u8;
        for m in [25u8, 50, 75] {
            if percent >= m && self.milestone < m {
                self.milestone = m;
                events.push(TraceEvent::Milestone(m));
            }
        }
        if self.next >= n {
            self.done = true;
            events.push(TraceEvent::Completed);
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_is_closed_apple_with_notch() {
        let p = apple_path([0.0, 0.0], 0.3);
        assert_eq!(p.points.len(), CHECKPOINTS);
        let first = p.points[0];
        let last = *p.points.last().unwrap();
        assert!(dist(first, last) < 0.3, "start and end meet near the stem");
        // Notch: points near the top are lower than the shoulders of the apple.
        let top_y = p.points.iter().map(|q| q[1]).fold(f64::MIN, f64::max);
        assert!(first[1] < top_y);
    }

    #[test]
    fn following_the_outline_completes_with_high_score() {
        let mut t = AppleTracer::new(apple_path([0.5, -0.5], 0.25));
        let pts = t.path.points.clone();
        let mut events = Vec::new();
        for p in pts.iter().chain(std::iter::once(&pts[pts.len() - 1])) {
            events.extend(t.update(*p, 0.1));
        }
        assert!(events.contains(&TraceEvent::Started));
        assert!(events.contains(&TraceEvent::Milestone(50)));
        assert!(events.contains(&TraceEvent::Completed));
        assert!(t.done);
        assert!(t.score() > 0.95, "score {}", t.score());
    }

    #[test]
    fn cannot_skip_ahead_or_score_without_tracing() {
        let mut t = AppleTracer::new(apple_path([0.5, -0.5], 0.25));
        // Not started: jumping to the middle does nothing.
        let mid = t.path.points[CHECKPOINTS / 2];
        t.update(mid, 0.1);
        assert!(!t.started);
        t.update(t.start_point(), 0.1);
        assert!(t.started);
        t.update(mid, 0.1);
        assert!(t.coverage() < 0.1, "lookahead prevents shortcuts");
        for _ in 0..50 {
            t.update([3.0, 3.0], 0.1);
        }
        assert!(t.accuracy() < 0.1);
        assert!(t.score() < 0.1);
    }
}
