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

    /// Score rounded to 0.1 %: the value that is saved and shown, and the one
    /// stars and unlocking use, so every screen agrees at the 60/80/90 % lines.
    pub fn final_score(&self) -> f64 {
        (self.score_percent() * 10.0).round() / 10.0
    }

    pub fn stars(&self) -> u8 {
        stars_for(self.final_score())
    }

    fn finish(&mut self, events: &mut Vec<GameEvent>) {
        self.stage = Stage::Finished;
        events.push(GameEvent::Finished {
            score: self.final_score(),
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

    /// The hand was not followed for a while (pause or sensor dropout): the
    /// next sample starts a new stroke rather than a slice across the gap.
    pub fn interrupt(&mut self) {
        // The apple tracer only tests single hand positions, never segments.
        if let Mode::Pnf(round) = &mut self.mode {
            round.interrupt();
        }
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
            score_percent: self.final_score(),
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

    #[test]
    fn star_thresholds_are_inclusive_at_sixty_eighty_and_ninety_percent() {
        for (score, stars) in [
            (f64::NAN, 0),
            (0.0, 0),
            (60.0f64.next_down(), 0),
            (60.0, 1),
            (UNLOCK_SCORE_PERCENT.next_down(), 1),
            (UNLOCK_SCORE_PERCENT, 2),
            (90.0f64.next_down(), 2),
            (90.0, 3),
            (100.0, 3),
        ] {
            assert_eq!(stars_for(score), stars, "{score}%");
        }
        assert_eq!(
            UNLOCK_SCORE_PERCENT, 80.0,
            "two stars is exactly the score that unlocks the next level"
        );
    }

    #[test]
    fn saved_score_stars_and_unlock_agree_at_the_rounding_boundary() {
        let mut game = Game::new(GameLevel::AppleTrace, Side::Right, None, 8, true, 1);
        if let Mode::Apple(round) = &mut game.mode {
            round.scores = vec![0.7996; APPLES_PER_ROUND as usize];
            round.done = APPLES_PER_ROUND;
        }
        assert!(
            game.score_percent() < UNLOCK_SCORE_PERCENT,
            "raw score {}",
            game.score_percent()
        );
        let s = game.to_session(&patient(), Utc::now());
        assert_eq!(s.score_percent, 80.0, "saved rounded to 0.1 %");
        assert_eq!(
            s.stars,
            stars_for(s.score_percent),
            "the saved stars match the saved score"
        );
        assert_eq!(game.stars(), 2, "the result screen and level menu agree");
        assert!(
            is_unlocked(&[&s], GameLevel::PnfD1, false),
            "a saved 80 % unlocks level 1"
        );
    }

    #[test]
    fn streak_ignores_undated_games_and_counts_a_busy_day_once() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 18).unwrap();
        let at = |days_ago: i64, hour: u32| {
            let d = today - chrono::Duration::days(days_ago);
            Some(
                Local
                    .from_local_datetime(&d.and_hms_opt(hour, 0, 0).unwrap())
                    .single()
                    .expect("unambiguous local time")
                    .with_timezone(&Utc),
            )
        };
        let game = |level, when: Option<DateTime<Utc>>| GameSession {
            recorded_at: when,
            ..session(level, 70.0, Utc::now())
        };
        let games = [
            game(GameLevel::AppleTrace, at(0, 8)),
            game(GameLevel::PnfD1, at(0, 12)),
            game(GameLevel::PnfD2, at(0, 21)),
            game(GameLevel::AppleTrace, at(1, 9)),
            game(GameLevel::PnfD1, at(1, 18)),
            game(GameLevel::AppleTrace, None),
            game(GameLevel::PnfD1, None),
            game(GameLevel::AppleTrace, at(3, 10)),
        ];
        let refs: Vec<&GameSession> = games.iter().collect();
        assert_eq!(
            streak_days(&refs, today),
            2,
            "five games over today and yesterday make a 2-day streak"
        );
        assert_eq!(streak_days(&refs[5..7], today), 0, "undated games only");
        assert_eq!(
            streak_days(&[&games[3], &games[5], &games[6]], today),
            1,
            "undated games do not count as playing today"
        );
        assert_eq!(
            streak_days(&[], NaiveDate::MIN),
            0,
            "no day before the first date"
        );
    }

    fn patient() -> Patient {
        Patient {
            id: "p1".into(),
            name: "Pat".into(),
            code: "PT-1".into(),
            arm: None,
            affected_side: None,
            updated_at: None,
        }
    }

    #[test]
    fn a_pause_or_sensor_dropout_never_becomes_one_long_slice() {
        let mut game = Game::new(GameLevel::PnfD2, Side::Right, Some(150.0), 2, false, 11);
        assert!(game.update([0.0, 0.0], 40.0, 3.1).contains(&GameEvent::Go));
        let path = match &game.mode {
            Mode::Pnf(round) => round.path,
            Mode::Apple(_) => unreachable!("level 2 is a PNF round"),
        };
        assert!(
            game.update(path.start, 40.0, 0.05)
                .contains(&GameEvent::RepStarted(1))
        );
        let clean_slices = |events: &[GameEvent]| {
            events
                .iter()
                .filter(|e| matches!(e, GameEvent::FruitSliced { clean: true, .. }))
                .count()
        };
        let mut joined = game.clone();
        assert_eq!(
            clean_slices(&joined.update(path.end, 120.0, 0.05)),
            3,
            "joined across the gap, one jump to the top cuts every fruit cleanly"
        );

        // Paused (or no sensor data) at the basket while the hand moves to the top.
        game.interrupt();
        let events = game.update(path.end, 120.0, 0.05);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, GameEvent::FruitSliced { .. })),
            "nothing is sliced across the gap: {events:?}"
        );
        assert!(
            events.contains(&GameEvent::ReachedTop),
            "the hand did reach the top"
        );
    }

    #[test]
    fn pnf_level_plays_through_the_game_loop_to_a_saved_session() {
        let mut game = Game::new(GameLevel::PnfD2, Side::Right, Some(150.0), 2, false, 11);
        assert!(
            game.update([0.0, 0.0], 40.0, 1.0).is_empty(),
            "countdown still running"
        );
        assert_eq!(game.stage, Stage::Countdown(2.0));
        assert!(game.update([0.0, 0.0], 40.0, 2.5).contains(&GameEvent::Go));
        assert_eq!(
            game.max_elevation, 0.0,
            "the countdown does not count elevation"
        );
        let path = match &game.mode {
            Mode::Pnf(round) => round.path,
            Mode::Apple(_) => unreachable!("level 2 is a PNF round"),
        };
        let mut events = Vec::new();
        for _ in 0..2 {
            events.extend(game.update(path.start, 100.0, 0.05));
            for i in 0..=60 {
                // Sensor glitches must not count as shoulder elevation.
                let glitch = if i % 2 == 0 { f64::NAN } else { f64::INFINITY };
                events.extend(game.update(path.point(f64::from(i) / 60.0), glitch, 0.05));
            }
            events.extend(game.update(path.end, 120.0, 0.05));
            for i in 0..=30 {
                events.extend(game.update(
                    lerp(path.end, path.start, f64::from(i) / 30.0),
                    100.0,
                    0.05,
                ));
            }
        }
        assert_eq!(game.stage, Stage::Finished);
        let count = |pred: fn(&GameEvent) -> bool| events.iter().filter(|e| pred(e)).count();
        assert_eq!(count(|e| matches!(e, GameEvent::RepStarted(_))), 2);
        assert_eq!(
            count(|e| matches!(e, GameEvent::FruitSliced { clean: true, .. })),
            6,
            "every fruit sliced cleanly"
        );
        assert_eq!(count(|e| matches!(e, GameEvent::ReachedTop)), 2);
        assert!(events.contains(&GameEvent::RepDone {
            clean: true,
            combo: 2
        }));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, GameEvent::Finished { stars: 3, .. })),
            "finished with three stars"
        );

        let s = game.to_session(&patient(), Utc::now());
        assert_eq!(
            (
                s.level,
                s.side,
                s.reps,
                s.clean_reps,
                s.bombs_hit,
                s.max_combo
            ),
            (GameLevel::PnfD2, Side::Right, 2, 2, 0, 2)
        );
        assert_eq!(s.completion_percent, 100.0);
        assert!(s.score_percent >= 95.0, "score {}", s.score_percent);
        assert!(
            s.accuracy_percent >= 95.0,
            "accuracy {}",
            s.accuracy_percent
        );
        assert_eq!(
            s.max_elevation_deg, 120.0,
            "non-finite elevation samples are ignored"
        );
        assert_eq!(s.points, game.points());
        assert_eq!(
            (s.patient_id.as_str(), s.source.as_str()),
            ("p1", SOURCE_DESKTOP)
        );
        assert!(s.duration_seconds > 0.0);

        assert!(
            game.update(path.start, 170.0, 0.05).is_empty(),
            "a finished game ignores input"
        );
        assert!(game.skip().is_empty(), "nothing to skip after the round");
        assert_eq!(game.max_elevation, 120.0);
    }

    #[test]
    fn skipping_apples_scores_partial_tracing_and_waits_for_the_pause() {
        let mut game = Game::new(GameLevel::AppleTrace, Side::Left, None, 8, true, 4);
        assert!(game.update([0.0, 0.0], 0.0, 3.1).contains(&GameEvent::Go));
        let pts = match &game.mode {
            Mode::Apple(round) => round.tracer.path.points.clone(),
            Mode::Pnf(_) => unreachable!("level 0 is the apple"),
        };
        for p in &pts[..pts.len() / 4] {
            game.update(*p, 60.0, 0.1);
        }
        let events = game.skip();
        let score = match events.as_slice() {
            [GameEvent::AppleSliced { score, .. }] => *score,
            other => panic!("expected one sliced apple, got {other:?}"),
        };
        let (coverage, tracer_score) = match &game.mode {
            Mode::Apple(round) => (round.tracer.coverage(), round.tracer.score()),
            Mode::Pnf(_) => unreachable!(),
        };
        assert_eq!(
            score, tracer_score,
            "a skipped apple keeps its tracing score"
        );
        assert!(
            coverage > 0.15 && coverage < 0.5,
            "partial outline traced: {coverage}"
        );
        assert!(score > 0.0 && score < 0.8, "partial credit: {score}");
        assert!(
            game.skip().is_empty(),
            "skip is ignored while the next apple is coming"
        );

        for apple in 2..=APPLES_PER_ROUND {
            for _ in 0..15 {
                game.update([3.0, 3.0], 60.0, 0.1);
            }
            let events = game.skip();
            assert!(
                matches!(events.first(), Some(GameEvent::AppleSliced { score, .. }) if *score == 0.0),
                "untraced apple {apple} scores zero: {events:?}"
            );
            assert_eq!(
                events
                    .iter()
                    .any(|e| matches!(e, GameEvent::Finished { .. })),
                apple == APPLES_PER_ROUND,
                "the round ends after the last apple"
            );
        }
        assert_eq!(game.stage, Stage::Finished);
        let expected = score / f64::from(APPLES_PER_ROUND) * 100.0;
        assert!(
            (game.score_percent() - expected).abs() < 1e-9,
            "skipped apples count as zero: {} vs {expected}",
            game.score_percent()
        );
        assert_eq!(game.stars(), 0);
        let s = game.to_session(&patient(), Utc::now());
        assert_eq!(
            (s.level, s.side, s.reps, s.clean_reps),
            (GameLevel::AppleTrace, Side::Left, 3, 0)
        );
        assert!(
            s.completion_percent < 20.0,
            "completion {}",
            s.completion_percent
        );
        assert!(game.skip().is_empty(), "nothing to skip after the round");
    }
}
