//! Fruit-Ninja-style PNF shoulder game: logic, scoring, unlocks and streaks.
//!
//! Coordinates are in arm lengths with the shoulder at the origin, x to the
//! patient's right (screen right, mirror view) and y up.

pub mod apple;
pub mod fx;
pub mod pnf;

use crate::clinical::Side;
use crate::model::{GameLevel, GameSession, Patient, SOURCE_DESKTOP, UNLOCK_SCORE_PERCENT, new_id};
use apple::{AppleTracer, TraceEvent, apple_path};
use chrono::{DateTime, Local, NaiveDate, Utc};
use pnf::{FruitKind, PnfEvent, PnfRound};
use std::collections::BTreeSet;

pub type Pt = [f64; 2];

pub fn add(a: Pt, b: Pt) -> Pt {
    [a[0] + b[0], a[1] + b[1]]
}

pub fn sub(a: Pt, b: Pt) -> Pt {
    [a[0] - b[0], a[1] - b[1]]
}

pub fn scale(a: Pt, s: f64) -> Pt {
    [a[0] * s, a[1] * s]
}

pub fn lerp(a: Pt, b: Pt, t: f64) -> Pt {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

pub fn dist(a: Pt, b: Pt) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Distance from point `p` to segment `a`-`b`.
pub fn seg_point_distance(a: Pt, b: Pt, p: Pt) -> f64 {
    let ab = sub(b, a);
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    if len2 < 1e-12 {
        return dist(a, p);
    }
    let t = (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0);
    dist(lerp(a, b, t), p)
}

pub const APPLES_PER_ROUND: u32 = 3;
const COUNTDOWN: f64 = 3.0;
const APPLE_PAUSE: f64 = 1.4;

/// Diagonal length factor from the patient's best abduction range.
pub fn reach_from_rom(max_abduction_deg: Option<f64>) -> f64 {
    max_abduction_deg.map_or(0.9, |e| ((e - 20.0) / 110.0).clamp(0.5, 1.0))
}

/// Apple centre and radius sized to the patient's abduction range.
pub fn apple_geometry(max_abduction_deg: Option<f64>, side: Side) -> (Pt, f64) {
    let rom = max_abduction_deg.unwrap_or(100.0).clamp(30.0, 180.0);
    let radius = (0.12 + 0.18 * ((rom - 30.0) / 120.0)).clamp(0.12, 0.30);
    let elevation = (rom * 0.5).clamp(15.0, 70.0).to_radians();
    let reach = 0.92 - radius;
    (
        [
            side.sign() * elevation.sin() * reach,
            -elevation.cos() * reach,
        ],
        radius,
    )
}

pub fn stars_for(score_percent: f64) -> u8 {
    match score_percent {
        s if s >= 90.0 => 3,
        s if s >= UNLOCK_SCORE_PERCENT => 2,
        s if s >= 60.0 => 1,
        _ => 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GameEvent {
    Go,
    TraceStarted,
    TraceMilestone(u8),
    AppleSliced {
        center: Pt,
        radius: f64,
        score: f64,
    },
    RepStarted(u32),
    FruitSliced {
        pos: Pt,
        kind: FruitKind,
        clean: bool,
    },
    BombHit(Pt),
    ReachedTop,
    RepDone {
        clean: bool,
        combo: u32,
    },
    Finished {
        score: f64,
        stars: u8,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    Countdown(f64),
    Playing,
    Finished,
}

#[derive(Clone, Debug)]
pub struct AppleRound {
    pub tracer: AppleTracer,
    pub done: u32,
    pub scores: Vec<f64>,
    pub accuracies: Vec<f64>,
    pub coverages: Vec<f64>,
    pub pause: f64,
    pub points: i64,
}

#[derive(Clone, Debug)]
pub enum Mode {
    Apple(AppleRound),
    Pnf(PnfRound),
}

#[derive(Clone, Debug)]
pub struct Game {
    pub level: GameLevel,
    pub side: Side,
    pub mode: Mode,
    pub stage: Stage,
    pub elapsed: f64,
    pub max_elevation: f64,
}

impl Game {
    pub fn new(
        level: GameLevel,
        side: Side,
        max_abduction_deg: Option<f64>,
        reps: u32,
        bombs: bool,
        seed: u64,
    ) -> Self {
        let mode = match level {
            GameLevel::AppleTrace => {
                let (center, radius) = apple_geometry(max_abduction_deg, side);
                Mode::Apple(AppleRound {
                    tracer: AppleTracer::new(apple_path(center, radius)),
                    done: 0,
                    scores: Vec::new(),
                    accuracies: Vec::new(),
                    coverages: Vec::new(),
                    pause: 0.0,
                    points: 0,
                })
            }
            _ => Mode::Pnf(PnfRound::new(
                level,
                side,
                reach_from_rom(max_abduction_deg),
                reps,
                bombs,
                seed,
            )),
        };
        Self {
            level,
            side,
            mode,
            stage: Stage::Countdown(COUNTDOWN),
            elapsed: 0.0,
            max_elevation: 0.0,
        }
    }

    pub fn points(&self) -> i64 {
        match &self.mode {
            Mode::Apple(a) => a.points,
            Mode::Pnf(r) => r.points,
        }
    }

    pub fn score_percent(&self) -> f64 {
        let fraction = match &self.mode {
            Mode::Apple(a) => a.scores.iter().sum::<f64>() / f64::from(APPLES_PER_ROUND),
            Mode::Pnf(r) => r.score_fraction(),
        };
        (fraction * 100.0).clamp(0.0, 100.0)
    }

    pub fn stars(&self) -> u8 {
        stars_for(self.score_percent())
    }

    fn finish(&mut self, events: &mut Vec<GameEvent>) {
        self.stage = Stage::Finished;
        events.push(GameEvent::Finished {
            score: self.score_percent(),
            stars: self.stars(),
        });
    }

    fn complete_apple(round: &mut AppleRound, events: &mut Vec<GameEvent>) {
        let score = round.tracer.score();
        round.scores.push(score);
        round.accuracies.push(round.tracer.accuracy());
        round.coverages.push(round.tracer.coverage());
        round.points += (score * 100.0).round() as i64;
        round.done += 1;
        round.pause = APPLE_PAUSE;
        events.push(GameEvent::AppleSliced {
            center: round.tracer.path.center,
            radius: round.tracer.path.radius,
            score,
        });
    }

    /// Give up on the current apple (Level 0) or end the round early (Levels 1-2).
    pub fn skip(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        if self.stage == Stage::Finished {
            return events;
        }
        match &mut self.mode {
            Mode::Apple(round) if round.pause <= 0.0 => {
                Self::complete_apple(round, &mut events);
                if round.done >= APPLES_PER_ROUND {
                    self.finish(&mut events);
                }
            }
            Mode::Apple(_) => {}
            Mode::Pnf(_) => self.finish(&mut events),
        }
        events
    }

    pub fn update(&mut self, hand: Pt, elevation_deg: f64, dt: f64) -> Vec<GameEvent> {
        let mut events = Vec::new();
        match self.stage {
            Stage::Finished => return events,
            Stage::Countdown(left) => {
                let left = left - dt;
                if left <= 0.0 {
                    self.stage = Stage::Playing;
                    events.push(GameEvent::Go);
                } else {
                    self.stage = Stage::Countdown(left);
                }
                return events;
            }
            Stage::Playing => {}
        }
        self.elapsed += dt;
        if elevation_deg.is_finite() {
            self.max_elevation = self.max_elevation.max(elevation_deg);
        }
        let mut finished = false;
        match &mut self.mode {
            Mode::Apple(round) => {
                if round.pause > 0.0 {
                    round.pause -= dt;
                    if round.pause <= 0.0 {
                        if round.done >= APPLES_PER_ROUND {
                            finished = true;
                        } else {
                            round.tracer = AppleTracer::new(round.tracer.path.clone());
                        }
                    }
                } else {
                    for e in round.tracer.update(hand, dt) {
                        match e {
                            TraceEvent::Started => events.push(GameEvent::TraceStarted),
                            TraceEvent::Milestone(m) => events.push(GameEvent::TraceMilestone(m)),
                            TraceEvent::Completed => Self::complete_apple(round, &mut events),
                        }
                    }
                }
            }
            Mode::Pnf(round) => {
                for e in round.update(hand, dt) {
                    events.push(match e {
                        PnfEvent::RepStarted(n) => GameEvent::RepStarted(n),
                        PnfEvent::Sliced { pos, kind, clean } => {
                            GameEvent::FruitSliced { pos, kind, clean }
                        }
                        PnfEvent::Bomb(p) => GameEvent::BombHit(p),
                        PnfEvent::ReachedTop => GameEvent::ReachedTop,
                        PnfEvent::RepDone { clean, combo } => GameEvent::RepDone { clean, combo },
                        PnfEvent::Finished => {
                            finished = true;
                            continue;
                        }
                    });
                }
            }
        }
        if finished {
            self.finish(&mut events);
        }
        events
    }

    pub fn to_session(&self, patient: &Patient, recorded_at: DateTime<Utc>) -> GameSession {
        let (reps, clean_reps, bombs_hit, max_combo, accuracy, completion) = match &self.mode {
            Mode::Apple(a) => {
                let n = f64::from(APPLES_PER_ROUND);
                (
                    a.done,
                    a.scores.iter().filter(|s| **s >= 0.9).count() as u32,
                    0,
                    0,
                    a.accuracies.iter().sum::<f64>() / n * 100.0,
                    a.coverages.iter().sum::<f64>() / n * 100.0,
                )
            }
            Mode::Pnf(r) => (
                r.rep_scores.len() as u32,
                r.clean_reps,
                r.bombs_hit,
                r.max_combo,
                r.session_accuracy() * 100.0,
                r.rep_scores.len() as f64 / f64::from(r.reps_total) * 100.0,
            ),
        };
        GameSession {
            id: new_id("game"),
            patient_id: patient.id.clone(),
            patient_name: patient.name.clone(),
            side: self.side,
            level: self.level,
            score_percent: (self.score_percent() * 10.0).round() / 10.0,
            points: self.points(),
            stars: self.stars(),
            reps,
            clean_reps,
            bombs_hit,
            max_combo,
            accuracy_percent: (accuracy * 10.0).round() / 10.0,
            completion_percent: (completion * 10.0).round() / 10.0,
            duration_seconds: (self.elapsed * 10.0).round() / 10.0,
            max_elevation_deg: (self.max_elevation * 10.0).round() / 10.0,
            recorded_at: Some(recorded_at),
            source: SOURCE_DESKTOP.to_string(),
        }
    }
}

pub fn best_score(games: &[&GameSession], level: GameLevel) -> Option<f64> {
    games
        .iter()
        .filter(|g| g.level == level)
        .map(|g| g.score_percent)
        .fold(None, |best, s| Some(best.map_or(s, |b: f64| b.max(s))))
}

/// A level unlocks once the previous level scored at least 80%.
pub fn is_unlocked(games: &[&GameSession], level: GameLevel, physio_override: bool) -> bool {
    physio_override
        || level
            .previous()
            .is_none_or(|prev| best_score(games, prev).is_some_and(|s| s >= UNLOCK_SCORE_PERCENT))
}

/// Consecutive local days with at least one game, ending today (or yesterday).
pub fn streak_days(games: &[&GameSession], today: NaiveDate) -> u32 {
    let days: BTreeSet<NaiveDate> = games
        .iter()
        .filter_map(|g| g.recorded_at)
        .map(|t| t.with_timezone(&Local).date_naive())
        .collect();
    let mut day = if days.contains(&today) {
        today
    } else {
        match today.pred_opt() {
            Some(d) => d,
            None => return 0,
        }
    };
    let mut streak = 0;
    while days.contains(&day) {
        streak += 1;
        match day.pred_opt() {
            Some(d) => day = d,
            None => break,
        }
    }
    streak
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn session(level: GameLevel, score: f64, when: DateTime<Utc>) -> GameSession {
        GameSession {
            id: new_id("g"),
            patient_id: "p".into(),
            patient_name: "P".into(),
            side: Side::Right,
            level,
            score_percent: score,
            points: 0,
            stars: stars_for(score),
            reps: 1,
            clean_reps: 0,
            bombs_hit: 0,
            max_combo: 0,
            accuracy_percent: 0.0,
            completion_percent: 0.0,
            duration_seconds: 0.0,
            max_elevation_deg: 0.0,
            recorded_at: Some(when),
            source: "test".into(),
        }
    }

    #[test]
    fn unlocks_follow_eighty_percent_rule() {
        let now = Utc::now();
        let low = session(GameLevel::AppleTrace, 79.9, now);
        let high = session(GameLevel::AppleTrace, 80.0, now);
        assert!(is_unlocked(&[], GameLevel::AppleTrace, false));
        assert!(!is_unlocked(&[&low], GameLevel::PnfD1, false));
        assert!(is_unlocked(&[&low, &high], GameLevel::PnfD1, false));
        assert!(!is_unlocked(&[&high], GameLevel::PnfD2, false));
        assert!(is_unlocked(&[], GameLevel::PnfD2, true));
    }

    #[test]
    fn streak_counts_consecutive_local_days() {
        let today = Local::now().date_naive();
        let at = |days_ago: i64| {
            let d = today - chrono::Duration::days(days_ago);
            Local
                .from_local_datetime(&d.and_hms_opt(12, 0, 0).unwrap())
                .unwrap()
                .with_timezone(&Utc)
        };
        let g: Vec<GameSession> = [0, 1, 2, 4]
            .into_iter()
            .map(|d| session(GameLevel::AppleTrace, 50.0, at(d)))
            .collect();
        let refs: Vec<&GameSession> = g.iter().collect();
        assert_eq!(streak_days(&refs, today), 3);
        assert_eq!(
            streak_days(&refs[1..], today),
            2,
            "yesterday keeps the streak alive"
        );
        assert_eq!(streak_days(&refs[3..], today), 0);
    }

    #[test]
    fn stars_and_geometry_are_sane() {
        assert_eq!(stars_for(95.0), 3);
        assert_eq!(stars_for(80.0), 2);
        assert_eq!(stars_for(61.0), 1);
        assert_eq!(stars_for(10.0), 0);
        for rom in [None, Some(30.0), Some(90.0), Some(170.0)] {
            for side in [Side::Left, Side::Right] {
                let (c, r) = apple_geometry(rom, side);
                assert!(dist(c, [0.0, 0.0]) + r <= 0.93, "apple stays reachable");
                assert!(c[0] * side.sign() > 0.0, "apple on the measured side");
            }
        }
        assert!(reach_from_rom(Some(40.0)) < reach_from_rom(Some(150.0)));
    }

    #[test]
    fn apple_round_completes_after_three_apples() {
        let mut game = Game::new(GameLevel::AppleTrace, Side::Right, Some(110.0), 8, true, 1);
        let mut events = game.update([0.0, 0.0], 0.0, 3.1);
        assert!(events.contains(&GameEvent::Go));
        for _ in 0..APPLES_PER_ROUND {
            let pts = match &game.mode {
                Mode::Apple(a) => a.tracer.path.points.clone(),
                Mode::Pnf(_) => unreachable!(),
            };
            for p in &pts {
                events.extend(game.update(*p, 70.0, 0.1));
            }
            events.extend(game.update(pts[pts.len() - 1], 70.0, 0.1));
            for _ in 0..20 {
                events.extend(game.update(pts[0], 70.0, 0.1));
            }
        }
        assert_eq!(game.stage, Stage::Finished);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, GameEvent::Finished { stars: 3, .. }))
        );
        let patient = Patient {
            id: "p1".into(),
            name: "Pat".into(),
            code: "PT-1".into(),
            arm: None,
            affected_side: None,
            updated_at: None,
        };
        let s = game.to_session(&patient, Utc::now());
        assert_eq!(s.reps, 3);
        assert!(s.score_percent > 90.0);
        assert_eq!(s.max_elevation_deg, 70.0);
    }

    #[test]
    fn skipping_ends_pnf_round_with_partial_score() {
        let mut game = Game::new(GameLevel::PnfD1, Side::Left, None, 8, false, 2);
        game.update([0.0, 0.0], 0.0, 3.1);
        let events = game.skip();
        assert!(matches!(
            events.last(),
            Some(GameEvent::Finished { stars: 0, .. })
        ));
        assert_eq!(game.score_percent(), 0.0);
    }
}
