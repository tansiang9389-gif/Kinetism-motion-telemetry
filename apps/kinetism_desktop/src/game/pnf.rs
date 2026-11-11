//! Levels 1-2: slice fruit along a PNF diagonal (D1 or D2), then return to
//! the basket. Bombs sit off the diagonal to discourage compensation.

use super::{Pt, add, dist, lerp, scale, seg_point_distance, sub};
use crate::clinical::Side;
use crate::model::GameLevel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FruitKind {
    Apple,
    Orange,
    Watermelon,
    Lemon,
    Plum,
}

impl FruitKind {
    pub const ALL: [FruitKind; 5] = [
        FruitKind::Apple,
        FruitKind::Orange,
        FruitKind::Watermelon,
        FruitKind::Lemon,
        FruitKind::Plum,
    ];
}

#[derive(Clone, Debug)]
pub struct Fruit {
    pub kind: FruitKind,
    pub pos: Pt,
    pub radius: f64,
    pub sliced: bool,
    pub spin: f64,
}

#[derive(Clone, Debug)]
pub struct Bomb {
    pub pos: Pt,
    pub radius: f64,
    pub hit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    GoToStart,
    Swipe,
    Return,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PnfEvent {
    RepStarted(u32),
    Sliced {
        pos: Pt,
        kind: FruitKind,
        clean: bool,
    },
    Bomb(Pt),
    ReachedTop,
    RepDone {
        clean: bool,
        combo: u32,
    },
    Finished,
}

/// Quadratic Bezier from the start (hip) zone to the end target.
#[derive(Clone, Copy, Debug)]
pub struct Diagonal {
    pub start: Pt,
    pub control: Pt,
    pub end: Pt,
}

impl Diagonal {
    pub fn point(&self, t: f64) -> Pt {
        let a = lerp(self.start, self.control, t);
        let b = lerp(self.control, self.end, t);
        lerp(a, b, t)
    }

    pub fn tangent(&self, t: f64) -> Pt {
        let d = add(
            scale(sub(self.control, self.start), 2.0 * (1.0 - t)),
            scale(sub(self.end, self.control), 2.0 * t),
        );
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-9);
        scale(d, 1.0 / len)
    }

    pub fn distance(&self, p: Pt) -> f64 {
        const N: usize = 32;
        (0..N)
            .map(|i| {
                seg_point_distance(
                    self.point(i as f64 / N as f64),
                    self.point((i + 1) as f64 / N as f64),
                    p,
                )
            })
            .fold(f64::INFINITY, f64::min)
    }
}

/// D1/D2 flexion paths for the given arm, in arm lengths with the shoulder at
/// the origin. `reach` (0.5-1.0) shortens the path for limited range.
pub fn diagonal(level: GameLevel, side: Side, reach: f64) -> Diagonal {
    let (start, end, bulge) = match level {
        // Same-side hip -> opposite ear.
        GameLevel::PnfD1 => ([0.30, -0.88], [-0.50, 0.24], 0.10),
        // Opposite hip -> up and out above the same shoulder.
        _ => ([-0.48, -0.80], [0.55, 0.72], -0.10),
    };
    let reach = reach.clamp(0.5, 1.0);
    let end = lerp(start, end, reach);
    let mid = lerp(start, end, 0.5);
    let dir = sub(end, start);
    let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt().max(1e-9);
    let normal = [-dir[1] / len, dir[0] / len];
    let control = add(mid, scale(normal, bulge * len));
    let s = side.sign();
    let m = |p: Pt| [p[0] * s, p[1]];
    Diagonal {
        start: m(start),
        control: m(control),
        end: m(end),
    }
}

pub const ZONE_RADIUS: f64 = 0.13;
const FRUIT_T: [f64; 3] = [0.38, 0.64, 0.9];
const SWIPE_TIMEOUT: f64 = 30.0;

#[derive(Clone, Debug)]
pub struct PnfRound {
    pub path: Diagonal,
    pub phase: Phase,
    pub fruits: Vec<Fruit>,
    pub bombs: Vec<Bomb>,
    pub rep: u32,
    pub reps_total: u32,
    pub combo: u32,
    pub max_combo: u32,
    pub clean_reps: u32,
    pub bombs_hit: u32,
    pub points: i64,
    pub rep_scores: Vec<f64>,
    pub rep_accuracy: Vec<f64>,
    pub with_bombs: bool,
    pub phase_time: f64,
    prev: Option<Pt>,
    dev_sum: f64,
    dev_n: u32,
    rep_bombs: u32,
    rep_clean_slices: u32,
    rng: fastrand::Rng,
}

impl PnfRound {
    pub fn new(
        level: GameLevel,
        side: Side,
        reach: f64,
        reps: u32,
        with_bombs: bool,
        seed: u64,
    ) -> Self {
        Self {
            path: diagonal(level, side, reach),
            phase: Phase::GoToStart,
            fruits: Vec::new(),
            bombs: Vec::new(),
            rep: 0,
            reps_total: reps.max(1),
            combo: 0,
            max_combo: 0,
            clean_reps: 0,
            bombs_hit: 0,
            points: 0,
            rep_scores: Vec::new(),
            rep_accuracy: Vec::new(),
            with_bombs,
            phase_time: 0.0,
            prev: None,
            dev_sum: 0.0,
            dev_n: 0,
            rep_bombs: 0,
            rep_clean_slices: 0,
            rng: fastrand::Rng::with_seed(seed),
        }
    }

    fn spawn_wave(&mut self) {
        let path = self.path;
        self.fruits = FRUIT_T
            .iter()
            .map(|&t| Fruit {
                kind: FruitKind::ALL[self.rng.usize(..FruitKind::ALL.len())],
                pos: path.point(t + (self.rng.f64() - 0.5) * 0.06),
                radius: 0.075,
                sliced: false,
                spin: self.rng.f64() * std::f64::consts::TAU,
            })
            .collect();
        self.bombs.clear();
        if self.with_bombs {
            let t = 0.4 + self.rng.f64() * 0.3;
            let p = path.point(t);
            let tan = path.tangent(t);
            let normal = [-tan[1], tan[0]];
            let side = if self.rng.bool() { 1.0 } else { -1.0 };
            self.bombs.push(Bomb {
                pos: add(p, scale(normal, side * 0.27)),
                radius: 0.07,
                hit: false,
            });
        }
    }

    pub fn score_fraction(&self) -> f64 {
        if self.rep_scores.is_empty() {
            0.0
        } else {
            self.rep_scores.iter().sum::<f64>() / self.reps_total as f64
        }
    }

    pub fn accuracy(&self) -> f64 {
        if self.dev_n == 0 {
            1.0
        } else {
            (1.0 - (self.dev_sum / self.dev_n as f64) / 0.25).clamp(0.0, 1.0)
        }
    }

    /// Mean path accuracy over completed reps.
    pub fn session_accuracy(&self) -> f64 {
        if self.rep_accuracy.is_empty() {
            0.0
        } else {
            self.rep_accuracy.iter().sum::<f64>() / self.rep_accuracy.len() as f64
        }
    }

    fn finish_rep(&mut self, events: &mut Vec<PnfEvent>) {
        let total = self.fruits.len().max(1) as f64;
        let sliced = self.fruits.iter().filter(|f| f.sliced).count() as f64;
        let accuracy = self.accuracy();
        self.rep_accuracy.push(accuracy);
        let rep_score =
            (0.6 * sliced / total + 0.4 * accuracy - 0.15 * self.rep_bombs as f64).clamp(0.0, 1.0);
        self.rep_scores.push(rep_score);
        let clean = sliced == total && self.rep_bombs == 0 && self.rep_clean_slices as f64 == total;
        if clean {
            self.combo += 1;
            self.clean_reps += 1;
            self.max_combo = self.max_combo.max(self.combo);
            self.points += 20 * i64::from(self.combo);
        } else {
            self.combo = 0;
        }
        events.push(PnfEvent::RepDone {
            clean,
            combo: self.combo,
        });
    }

    pub fn update(&mut self, hand: Pt, dt: f64) -> Vec<PnfEvent> {
        let mut events = Vec::new();
        self.phase_time += dt;
        match self.phase {
            Phase::GoToStart => {
                if dist(hand, self.path.start) <= ZONE_RADIUS {
                    self.rep += 1;
                    self.spawn_wave();
                    self.dev_sum = 0.0;
                    self.dev_n = 0;
                    self.rep_bombs = 0;
                    self.rep_clean_slices = 0;
                    self.phase = Phase::Swipe;
                    self.phase_time = 0.0;
                    events.push(PnfEvent::RepStarted(self.rep));
                }
            }
            Phase::Swipe => {
                let from = self.prev.unwrap_or(hand);
                let motion = sub(hand, from);
                let moved = (motion[0] * motion[0] + motion[1] * motion[1]).sqrt();
                for fruit in self.fruits.iter_mut().filter(|f| !f.sliced) {
                    if seg_point_distance(from, hand, fruit.pos) <= fruit.radius + 0.02 {
                        fruit.sliced = true;
                        let along = if moved > 1e-6 {
                            let t = self.path.tangent(0.6);
                            (motion[0] * t[0] + motion[1] * t[1]) / moved
                        } else {
                            1.0
                        };
                        let clean = along > 0.3;
                        if clean {
                            self.rep_clean_slices += 1;
                        }
                        self.points += if clean { 15 } else { 8 };
                        events.push(PnfEvent::Sliced {
                            pos: fruit.pos,
                            kind: fruit.kind,
                            clean,
                        });
                    }
                }
                for bomb in self.bombs.iter_mut().filter(|b| !b.hit) {
                    if seg_point_distance(from, hand, bomb.pos) <= bomb.radius {
                        bomb.hit = true;
                        self.rep_bombs += 1;
                        self.bombs_hit += 1;
                        self.points -= 20;
                        events.push(PnfEvent::Bomb(bomb.pos));
                    }
                }
                self.dev_sum += self.path.distance(hand);
                self.dev_n += 1;
                if dist(hand, self.path.end) <= ZONE_RADIUS || self.phase_time > SWIPE_TIMEOUT {
                    events.push(PnfEvent::ReachedTop);
                    self.phase = Phase::Return;
                    self.phase_time = 0.0;
                }
            }
            Phase::Return => {
                if dist(hand, self.path.start) <= ZONE_RADIUS || self.phase_time > SWIPE_TIMEOUT {
                    self.points += 5;
                    self.finish_rep(&mut events);
                    self.fruits.clear();
                    self.bombs.clear();
                    self.phase_time = 0.0;
                    if self.rep >= self.reps_total {
                        self.phase = Phase::Done;
                        events.push(PnfEvent::Finished);
                    } else {
                        self.phase = Phase::GoToStart;
                    }
                }
            }
            Phase::Done => {}
        }
        self.prev = Some(hand);
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_rep(round: &mut PnfRound, offset: f64) -> Vec<PnfEvent> {
        let mut events = round.update(round.path.start, 0.05);
        let tan_mid = round.path.tangent(0.5);
        let normal = [-tan_mid[1], tan_mid[0]];
        for i in 0..=60 {
            let p = add(round.path.point(i as f64 / 60.0), scale(normal, offset));
            events.extend(round.update(p, 0.05));
        }
        events.extend(round.update(round.path.end, 0.05));
        for i in 0..=30 {
            events.extend(round.update(
                lerp(round.path.end, round.path.start, i as f64 / 30.0),
                0.05,
            ));
        }
        events
    }

    #[test]
    fn clean_swipes_slice_everything_and_build_combo() {
        let mut round = PnfRound::new(GameLevel::PnfD2, Side::Right, 1.0, 2, false, 7);
        let e1 = run_rep(&mut round, 0.0);
        assert_eq!(
            e1.iter()
                .filter(|e| matches!(e, PnfEvent::Sliced { clean: true, .. }))
                .count(),
            3
        );
        assert!(e1.contains(&PnfEvent::RepDone {
            clean: true,
            combo: 1
        }));
        let e2 = run_rep(&mut round, 0.0);
        assert!(e2.contains(&PnfEvent::RepDone {
            clean: true,
            combo: 2
        }));
        assert!(e2.contains(&PnfEvent::Finished));
        assert_eq!(round.phase, Phase::Done);
        assert!(
            round.score_fraction() > 0.95,
            "score {}",
            round.score_fraction()
        );
        assert_eq!(round.max_combo, 2);
    }

    #[test]
    fn off_path_swipes_score_lower_and_can_hit_bombs() {
        let mut round = PnfRound::new(GameLevel::PnfD1, Side::Left, 1.0, 1, true, 3);
        round.update(round.path.start, 0.05);
        let bomb = round.bombs[0].pos;
        // Swipe straight through the bomb instead of along the diagonal.
        let events: Vec<_> = [round.path.start, bomb, round.path.end]
            .into_iter()
            .flat_map(|p| round.update(p, 0.05))
            .collect();
        assert!(events.iter().any(|e| matches!(e, PnfEvent::Bomb(_))));
        assert_eq!(round.bombs_hit, 1);
        // Back to the basket finishes the rep: not clean, combo broken.
        let events = round.update(round.path.start, 0.05);
        assert!(events.iter().any(|e| matches!(
            e,
            PnfEvent::RepDone {
                clean: false,
                combo: 0
            }
        )));
        assert!(round.score_fraction() < 0.9);
    }

    #[test]
    fn diagonals_cross_the_body_and_mirror_by_side() {
        let d1 = diagonal(GameLevel::PnfD1, Side::Right, 1.0);
        assert!(
            d1.start[0] > 0.0 && d1.end[0] < 0.0,
            "D1 ends across the midline"
        );
        let d2 = diagonal(GameLevel::PnfD2, Side::Right, 1.0);
        assert!(
            d2.start[0] < 0.0 && d2.end[0] > 0.0 && d2.end[1] > 0.5,
            "D2 ends up and out"
        );
        let d2_left = diagonal(GameLevel::PnfD2, Side::Left, 1.0);
        assert!((d2_left.end[0] + d2.end[0]).abs() < 1e-9);
        let short = diagonal(GameLevel::PnfD2, Side::Right, 0.5);
        assert!(short.end[1] < d2.end[1]);
    }

    #[test]
    fn stalled_swipes_time_out_and_crosswise_slices_are_not_clean() {
        let mut round = PnfRound::new(GameLevel::PnfD1, Side::Right, 1.0, 0, false, 5);
        assert_eq!(
            round.reps_total, 1,
            "a zero-rep setting still plays one rep"
        );
        assert!(
            round.update([2.0, 2.0], 60.0).is_empty(),
            "no timeout while waiting at the basket"
        );
        assert_eq!(round.phase, Phase::GoToStart);
        assert_eq!(
            round.update(round.path.start, 0.05),
            vec![PnfEvent::RepStarted(1)]
        );

        // Cut across the middle fruit, perpendicular to the diagonal.
        let fruit = round.fruits[1].pos;
        let tangent = round.path.tangent(0.6);
        let normal = [-tangent[1], tangent[0]];
        let (from, to) = (
            sub(fruit, scale(normal, 0.3)),
            add(fruit, scale(normal, 0.3)),
        );
        let events = round.update(from, 0.05);
        assert!(
            events.is_empty(),
            "moving off the diagonal slices nothing: {events:?}"
        );
        let events = round.update(to, 0.05);
        assert!(
            matches!(events.as_slice(), [PnfEvent::Sliced { clean: false, .. }]),
            "a crosswise cut slices but is not clean: {events:?}"
        );
        assert_eq!(round.points, 8, "unclean slices earn fewer points");

        // The patient stalls: the round moves on after the swipe timeout.
        assert_eq!(round.update(to, 31.0), vec![PnfEvent::ReachedTop]);
        assert_eq!(round.phase, Phase::Return);
        assert!(
            round.update(to, 10.0).is_empty(),
            "still waiting for the basket"
        );
        assert_eq!(
            round.update(to, 21.0),
            vec![
                PnfEvent::RepDone {
                    clean: false,
                    combo: 0
                },
                PnfEvent::Finished
            ]
        );
        assert_eq!(round.phase, Phase::Done);
        assert_eq!(round.points, 13, "unclean slice plus the return bonus");
        assert_eq!(round.clean_reps, 0);
        assert!(
            round.rep_scores[0] < 0.6,
            "one of three fruit and an off-path swipe score low: {}",
            round.rep_scores[0]
        );
        assert!(
            round.update(round.path.start, 1.0).is_empty(),
            "a finished round ignores input"
        );
    }
}
