//! Visual-only effects: blade trail, juice, flying halves, splats and popups.

use super::Pt;
use super::pnf::FruitKind;
use std::collections::VecDeque;

const TRAIL_SECONDS: f64 = 0.4;
const GRAVITY: f64 = -3.2;

/// (skin, flesh, juice) RGB.
pub fn fruit_colors(kind: FruitKind) -> ([u8; 3], [u8; 3], [u8; 3]) {
    match kind {
        FruitKind::Apple => ([214, 40, 40], [255, 244, 214], [255, 214, 120]),
        FruitKind::Orange => ([245, 140, 20], [255, 190, 70], [255, 160, 40]),
        FruitKind::Watermelon => ([46, 140, 60], [235, 60, 70], [240, 60, 80]),
        FruitKind::Lemon => ([250, 214, 40], [255, 245, 160], [250, 230, 90]),
        FruitKind::Plum => ([130, 40, 140], [240, 190, 90], [200, 70, 170]),
    }
}

#[derive(Clone, Debug)]
pub struct Particle {
    pub pos: Pt,
    pub vel: Pt,
    pub color: [u8; 3],
    pub life: f64,
    pub max_life: f64,
    pub size: f64,
}

#[derive(Clone, Debug)]
pub struct Half {
    pub pos: Pt,
    pub vel: Pt,
    pub angle: f64,
    pub spin: f64,
    pub kind: FruitKind,
    pub radius: f64,
    /// Which half of the fruit (+1 / -1) relative to the cut line.
    pub side: f64,
    pub life: f64,
}

#[derive(Clone, Debug)]
pub struct Splat {
    pub pos: Pt,
    pub color: [u8; 3],
    pub radius: f64,
    pub life: f64,
}

#[derive(Clone, Debug)]
pub struct Popup {
    pub pos: Pt,
    pub text: String,
    pub color: [u8; 3],
    pub life: f64,
    pub big: bool,
}

#[derive(Clone, Debug)]
pub struct Blast {
    pub pos: Pt,
    pub life: f64,
}

#[derive(Clone, Debug)]
pub struct Fx {
    pub trail: VecDeque<(Pt, f64)>,
    pub particles: Vec<Particle>,
    pub halves: Vec<Half>,
    pub splats: Vec<Splat>,
    pub popups: Vec<Popup>,
    pub blasts: Vec<Blast>,
    pub shake: f64,
    pub time: f64,
    rng: fastrand::Rng,
}

impl Default for Fx {
    fn default() -> Self {
        Self {
            trail: VecDeque::new(),
            particles: Vec::new(),
            halves: Vec::new(),
            splats: Vec::new(),
            popups: Vec::new(),
            blasts: Vec::new(),
            shake: 0.0,
            time: 0.0,
            rng: fastrand::Rng::with_seed(42),
        }
    }
}

impl Fx {
    pub fn add_trail(&mut self, p: Pt) {
        self.trail.push_back((p, self.time));
    }

    pub fn update(&mut self, dt: f64) {
        self.time += dt;
        let now = self.time;
        while self
            .trail
            .front()
            .is_some_and(|(_, t)| now - t > TRAIL_SECONDS)
        {
            self.trail.pop_front();
        }
        for p in &mut self.particles {
            p.vel[1] += GRAVITY * dt;
            p.pos[0] += p.vel[0] * dt;
            p.pos[1] += p.vel[1] * dt;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
        for h in &mut self.halves {
            h.vel[1] += GRAVITY * dt;
            h.pos[0] += h.vel[0] * dt;
            h.pos[1] += h.vel[1] * dt;
            h.angle += h.spin * dt;
            h.life -= dt;
        }
        self.halves.retain(|h| h.life > 0.0);
        for s in &mut self.splats {
            s.life -= dt;
        }
        self.splats.retain(|s| s.life > 0.0);
        for p in &mut self.popups {
            p.pos[1] += 0.25 * dt;
            p.life -= dt;
        }
        self.popups.retain(|p| p.life > 0.0);
        for b in &mut self.blasts {
            b.life -= dt;
        }
        self.blasts.retain(|b| b.life > 0.0);
        self.shake = (self.shake - dt * 2.5).max(0.0);
    }

    /// Slice a fruit: two halves fly apart along the cut, juice sprays.
    pub fn slice(&mut self, pos: Pt, kind: FruitKind, radius: f64, cut_angle: f64) {
        let (_, _, juice) = fruit_colors(kind);
        let normal = [-cut_angle.sin(), cut_angle.cos()];
        for side in [1.0, -1.0] {
            self.halves.push(Half {
                pos,
                vel: [normal[0] * side * 0.45, 0.35 + normal[1] * side * 0.45],
                angle: cut_angle,
                spin: side * (1.5 + self.rng.f64() * 2.0),
                kind,
                radius,
                side,
                life: 1.6,
            });
        }
        for _ in 0..26 {
            let a = self.rng.f64() * std::f64::consts::TAU;
            let speed = 0.3 + self.rng.f64() * 1.1;
            self.particles.push(Particle {
                pos,
                vel: [a.cos() * speed, a.sin() * speed + 0.4],
                color: juice,
                life: 0.5 + self.rng.f64() * 0.6,
                max_life: 1.1,
                size: 0.008 + self.rng.f64() * 0.016,
            });
        }
        for _ in 0..5 {
            let a = self.rng.f64() * std::f64::consts::TAU;
            let d = radius * 0.9 * self.rng.f64();
            self.splats.push(Splat {
                pos: [pos[0] + a.cos() * d, pos[1] + a.sin() * d],
                color: juice,
                radius: radius * (0.25 + self.rng.f64() * 0.4),
                life: 3.0,
            });
        }
    }

    pub fn bomb(&mut self, pos: Pt) {
        self.blasts.push(Blast { pos, life: 0.8 });
        self.shake = 1.0;
        for _ in 0..30 {
            let a = self.rng.f64() * std::f64::consts::TAU;
            let speed = 0.6 + self.rng.f64() * 1.4;
            self.particles.push(Particle {
                pos,
                vel: [a.cos() * speed, a.sin() * speed],
                color: if self.rng.bool() {
                    [255, 170, 40]
                } else {
                    [90, 90, 90]
                },
                life: 0.4 + self.rng.f64() * 0.5,
                max_life: 0.9,
                size: 0.01 + self.rng.f64() * 0.02,
            });
        }
    }

    pub fn popup(&mut self, pos: Pt, text: impl Into<String>, color: [u8; 3], big: bool) {
        self.popups.push(Popup {
            pos,
            text: text.into(),
            color,
            life: if big { 1.6 } else { 1.0 },
            big,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_expire() {
        let mut fx = Fx::default();
        fx.add_trail([0.0, 0.0]);
        fx.slice([0.1, 0.2], FruitKind::Watermelon, 0.08, 0.5);
        fx.bomb([0.0, 0.0]);
        fx.popup([0.0, 0.0], "+15", [255, 255, 255], false);
        assert_eq!(fx.halves.len(), 2);
        for _ in 0..400 {
            fx.update(0.016);
        }
        assert!(fx.trail.is_empty() && fx.particles.is_empty() && fx.halves.is_empty());
        assert!(fx.splats.is_empty() && fx.popups.is_empty() && fx.blasts.is_empty());
        assert_eq!(fx.shake, 0.0);
    }
}
