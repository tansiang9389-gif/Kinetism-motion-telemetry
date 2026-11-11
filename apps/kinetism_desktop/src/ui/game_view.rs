//! Fruit-Ninja-style PNF game: level menu, canvas rendering and event effects.

use crate::app::{KinetismApp, Screen};
use crate::clinical::{Movement, Side};
use crate::game::fx::{Fx, fruit_colors};
use crate::game::pnf::{FruitKind, Phase, ZONE_RADIUS};
use crate::game::{Game, GameEvent, Mode, Pt, Stage, best_score, is_unlocked, streak_days};
use crate::model::{GameLevel, UNLOCK_SCORE_PERCENT};
use crate::sim::SimInput;
use crate::theme;
use crate::ui::{self, card};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, RichText, Sense, Shape,
    Stroke, Vec2,
};

struct View {
    rect: Rect,
    center: Pt,
    scale: f32,
    shake: Vec2,
}

impl View {
    fn new(rect: Rect, side: Side, shake: f64, time: f64) -> Self {
        let half = [1.22f32, 1.12f32];
        let scale = (rect.width() / (2.0 * half[0])).min(rect.height() / (2.0 * half[1]));
        let s = (shake * 9.0) as f32;
        Self {
            rect,
            center: [side.sign() * 0.04, -0.12],
            scale,
            shake: Vec2::new(
                (time * 53.0).sin() as f32 * s,
                (time * 71.0).cos() as f32 * s,
            ),
        }
    }

    fn pos(&self, p: Pt) -> Pos2 {
        Pos2::new(
            self.rect.center().x + (p[0] - self.center[0]) as f32 * self.scale,
            self.rect.center().y - (p[1] - self.center[1]) as f32 * self.scale,
        ) + self.shake
    }

    fn len(&self, l: f64) -> f32 {
        l as f32 * self.scale
    }

    fn game(&self, pos: Pos2) -> Pt {
        [
            f64::from((pos.x - self.rect.center().x) / self.scale) + self.center[0],
            f64::from(-(pos.y - self.rect.center().y) / self.scale) + self.center[1],
        ]
    }
}

fn rgba(c: [u8; 3], a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], (a.clamp(0.0, 1.0) * 255.0) as u8)
}

fn wood_background(p: &Painter, rect: Rect) {
    p.rect_filled(
        rect,
        CornerRadius::same(14),
        Color32::from_rgb(0x4A, 0x2E, 0x1C),
    );
    let planks = 7;
    let w = rect.width() / planks as f32;
    for i in 0..planks {
        let r = Rect::from_min_size(
            Pos2::new(rect.left() + i as f32 * w, rect.top()),
            Vec2::new(w, rect.height()),
        );
        let shade = if i % 2 == 0 {
            Color32::from_rgb(0x5A, 0x38, 0x22)
        } else {
            Color32::from_rgb(0x52, 0x33, 0x1F)
        };
        p.rect_filled(r.shrink2(Vec2::new(1.5, 0.0)), CornerRadius::ZERO, shade);
        for g in 0..5 {
            let x = r.left() + w * (0.18 + 0.17 * g as f32);
            let pts: Vec<Pos2> = (0..=24)
                .map(|k| {
                    let y = rect.top() + rect.height() * k as f32 / 24.0;
                    Pos2::new(
                        x + ((k as f32 * 0.7 + g as f32 * 1.9 + i as f32).sin()) * 3.0,
                        y,
                    )
                })
                .collect();
            p.add(Shape::line(
                pts,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(30, 16, 8, 60)),
            ));
        }
        p.vline(
            r.left(),
            rect.y_range(),
            Stroke::new(2.0, Color32::from_rgb(0x2E, 0x1B, 0x10)),
        );
    }
    let edge = Color32::from_black_alpha(110);
    for (dir, r) in [
        (
            egui::epaint::Direction::TopDown,
            Rect::from_min_size(rect.min, Vec2::new(rect.width(), 40.0)),
        ),
        (
            egui::epaint::Direction::BottomUp,
            Rect::from_min_size(
                Pos2::new(rect.left(), rect.bottom() - 40.0),
                Vec2::new(rect.width(), 40.0),
            ),
        ),
    ] {
        p.add(Shape::gradient_rect(r, dir, [edge, Color32::TRANSPARENT]));
    }
}

fn ghost_body(p: &Painter, v: &View, side: Side) {
    let s = side.sign();
    let mid = -s * 0.33;
    let body = Color32::from_rgba_unmultiplied(255, 255, 255, 16);
    p.circle_filled(v.pos([mid, 0.42]), v.len(0.17), body);
    p.add(Shape::convex_polygon(
        vec![
            v.pos([mid - 0.33, 0.0]),
            v.pos([mid + 0.33, 0.0]),
            v.pos([mid + 0.26, -1.05]),
            v.pos([mid - 0.26, -1.05]),
        ],
        body,
        Stroke::NONE,
    ));
    let other = [2.0 * mid, 0.0];
    p.line_segment(
        [v.pos(other), v.pos([other[0], -0.98])],
        Stroke::new(v.len(0.09), body),
    );
}

fn draw_fruit(p: &Painter, v: &View, pos: Pt, kind: FruitKind, radius: f64, t: f64) {
    let (skin, flesh, _) = fruit_colors(kind);
    let c = v.pos([pos[0], pos[1] + (t * 2.4 + pos[0] * 7.0).sin() * 0.008]);
    let r = v.len(radius);
    p.circle_filled(
        c + Vec2::new(r * 0.12, r * 0.18),
        r,
        Color32::from_black_alpha(70),
    );
    match kind {
        FruitKind::Lemon => {
            p.add(Shape::ellipse_filled(
                c,
                Vec2::new(r * 1.12, r * 0.86),
                theme::rgb(skin),
            ));
        }
        _ => {
            p.circle_filled(c, r, theme::rgb(skin));
        }
    }
    if kind == FruitKind::Watermelon {
        for k in -2..=2 {
            let x = c.x + k as f32 * r * 0.36;
            let h = (r * r - (x - c.x).powi(2)).max(0.0).sqrt();
            p.line_segment(
                [Pos2::new(x, c.y - h * 0.92), Pos2::new(x, c.y + h * 0.92)],
                Stroke::new(r * 0.12, Color32::from_rgb(0x1E, 0x6B, 0x2E)),
            );
        }
    }
    if kind == FruitKind::Orange {
        for k in 0..6 {
            let a = k as f32 * 1.1;
            p.circle_filled(
                c + Vec2::new(a.cos(), a.sin()) * r * 0.55,
                r * 0.05,
                theme::rgb(flesh),
            );
        }
    }
    if kind == FruitKind::Apple || kind == FruitKind::Plum {
        p.line_segment(
            [
                c + Vec2::new(0.0, -r * 0.8),
                c + Vec2::new(r * 0.12, -r * 1.25),
            ],
            Stroke::new(r * 0.12, Color32::from_rgb(0x5D, 0x3A, 0x1A)),
        );
        p.add(Shape::ellipse_filled(
            c + Vec2::new(r * 0.38, -r * 1.12),
            Vec2::new(r * 0.3, r * 0.14),
            Color32::from_rgb(0x4C, 0xAF, 0x50),
        ));
    }
    p.add(Shape::ellipse_filled(
        c + Vec2::new(-r * 0.35, -r * 0.4),
        Vec2::new(r * 0.22, r * 0.14),
        Color32::from_white_alpha(110),
    ));
}

fn draw_half(p: &Painter, v: &View, h: &crate::game::fx::Half) {
    let (skin, flesh, _) = fruit_colors(h.kind);
    let c = v.pos(h.pos);
    let r = v.len(h.radius);
    let alpha = (h.life / 1.6).min(1.0) as f32;
    let base = -(h.angle as f32)
        + if h.side > 0.0 {
            0.0
        } else {
            std::f32::consts::PI
        };
    let arc: Vec<Pos2> = (0..=16)
        .map(|i| {
            let a = base + std::f32::consts::PI * i as f32 / 16.0;
            c + Vec2::new(a.cos(), -a.sin()) * r
        })
        .collect();
    p.add(Shape::convex_polygon(
        arc.clone(),
        rgba(flesh, alpha),
        Stroke::NONE,
    ));
    p.add(Shape::line(arc, Stroke::new(r * 0.22, rgba(skin, alpha))));
}

fn draw_bomb(p: &Painter, v: &View, pos: Pt, radius: f64, t: f64) {
    let c = v.pos(pos);
    let r = v.len(radius);
    p.circle_filled(
        c + Vec2::new(r * 0.12, r * 0.18),
        r,
        Color32::from_black_alpha(90),
    );
    p.circle_filled(c, r, Color32::from_rgb(0x1E, 0x1E, 0x24));
    p.circle_stroke(c, r, Stroke::new(2.0, Color32::from_rgb(0xE0, 0x3C, 0x3C)));
    p.add(Shape::ellipse_filled(
        c + Vec2::new(-r * 0.35, -r * 0.35),
        Vec2::new(r * 0.2, r * 0.12),
        Color32::from_white_alpha(70),
    ));
    let fuse = c + Vec2::new(r * 0.55, -r * 1.05);
    p.line_segment(
        [c + Vec2::new(r * 0.3, -r * 0.8), fuse],
        Stroke::new(r * 0.12, Color32::from_rgb(0x8D, 0x6E, 0x63)),
    );
    let flicker = 0.7 + 0.3 * (t * 23.0).sin() as f32;
    p.circle_filled(
        fuse,
        r * 0.28 * flicker,
        Color32::from_rgb(0xFF, 0xC1, 0x07),
    );
    p.circle_filled(fuse, r * 0.14, Color32::WHITE);
    let red = Stroke::new(r * 0.16, Color32::from_rgb(0xE0, 0x3C, 0x3C));
    let k = r * 0.38;
    p.line_segment([c + Vec2::new(-k, -k), c + Vec2::new(k, k)], red);
    p.line_segment([c + Vec2::new(-k, k), c + Vec2::new(k, -k)], red);
}

fn draw_trail(p: &Painter, v: &View, fx: &Fx) {
    let pts: Vec<(Pos2, f64)> = fx
        .trail
        .iter()
        .map(|(pt, t)| (v.pos(*pt), fx.time - t))
        .collect();
    for w in pts.windows(2) {
        let age = w[1].1.max(0.0) as f32;
        let k = (1.0 - age / 0.4).clamp(0.0, 1.0);
        p.line_segment(
            [w[0].0, w[1].0],
            Stroke::new(
                18.0 * k + 2.0,
                Color32::from_rgba_unmultiplied(87, 219, 196, (90.0 * k) as u8),
            ),
        );
        p.line_segment(
            [w[0].0, w[1].0],
            Stroke::new(
                6.0 * k + 1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, (230.0 * k) as u8),
            ),
        );
    }
}

fn draw_fx(p: &Painter, v: &View, fx: &Fx) {
    for h in &fx.halves {
        draw_half(p, v, h);
    }
    for part in &fx.particles {
        let a = (part.life / part.max_life) as f32;
        p.circle_filled(
            v.pos(part.pos),
            v.len(part.size).max(1.5),
            rgba(part.color, a),
        );
    }
    for b in &fx.blasts {
        let k = 1.0 - (b.life / 0.8) as f32;
        let c = v.pos(b.pos);
        p.circle_filled(
            c,
            v.len(0.05 + 0.3 * f64::from(k)),
            Color32::from_rgba_unmultiplied(255, 200, 80, ((1.0 - k) * 140.0) as u8),
        );
        p.circle_stroke(
            c,
            v.len(0.08 + 0.45 * f64::from(k)),
            Stroke::new(
                4.0 * (1.0 - k) + 1.0,
                Color32::from_rgba_unmultiplied(255, 120, 40, ((1.0 - k) * 255.0) as u8),
            ),
        );
    }
    for pop in &fx.popups {
        let a = (pop.life / if pop.big { 1.6 } else { 1.0 }).min(1.0) as f32;
        let size = if pop.big { 40.0 } else { 24.0 };
        let c = v.pos(pop.pos);
        p.text(
            c + Vec2::new(2.0, 2.0),
            Align2::CENTER_CENTER,
            &pop.text,
            FontId::proportional(size),
            Color32::from_black_alpha((a * 160.0) as u8),
        );
        p.text(
            c,
            Align2::CENTER_CENTER,
            &pop.text,
            FontId::proportional(size),
            rgba(pop.color, a),
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ZoneKind {
    Basket,
    Target,
}

fn zone(p: &Painter, v: &View, at: Pt, label: &str, color: Color32, pulse: f32, kind: ZoneKind) {
    let c = v.pos(at);
    let r = v.len(ZONE_RADIUS);
    p.circle_filled(c, r * (1.0 + 0.08 * pulse), color.gamma_multiply(0.18));
    p.circle_stroke(c, r, Stroke::new(2.5, color));
    match kind {
        ZoneKind::Basket => {
            let b =
                Rect::from_center_size(c + Vec2::new(0.0, r * 0.15), Vec2::new(r * 1.1, r * 0.7));
            p.rect_filled(
                b,
                CornerRadius {
                    nw: 2,
                    ne: 2,
                    sw: 10,
                    se: 10,
                },
                Color32::from_rgb(0x9C, 0x6B, 0x3C),
            );
            for k in 1..4 {
                let x = b.left() + b.width() * k as f32 / 4.0;
                p.vline(
                    x,
                    b.y_range(),
                    Stroke::new(1.5, Color32::from_rgb(0x6D, 0x45, 0x22)),
                );
            }
        }
        ZoneKind::Target => ui::paint_star(p, c, r * (0.55 + 0.08 * pulse), color),
    }
    p.text(
        c + Vec2::new(0.0, r + 14.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(15.0),
        color,
    );
}

impl KinetismApp {
    pub(crate) fn play_screen(&mut self, ui: &mut egui::Ui) {
        let Some(patient) = self.current_patient() else {
            self.screen = Screen::Patients;
            return;
        };
        if self.play.game.is_some() {
            self.game_canvas(ui);
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| self.level_menu(ui, &patient));
    }

    fn level_menu(&mut self, ui: &mut egui::Ui, patient: &crate::model::Patient) {
        ui::eyebrow(ui, "Fruit PNF game");
        ui::title(ui, "Choose a level");
        let games = self.store.games_for(&patient.id);
        let streak = streak_days(&games, chrono::Local::now().date_naive());
        let override_all = self.store.settings.physio_unlock_all;
        let status: Vec<(GameLevel, bool, Option<f64>)> = GameLevel::ALL
            .into_iter()
            .map(|l| {
                (
                    l,
                    is_unlocked(&games, l, override_all),
                    best_score(&games, l),
                )
            })
            .collect();
        drop(games);
        ui.horizontal(|ui| {
            ui::chip(
                ui,
                &format!("🔥 {streak}-day streak"),
                if streak > 0 {
                    theme::WARN
                } else {
                    theme::FAINT
                },
            );
            ui::chip(ui, &format!("{} arm", self.side().label()), theme::MUTED);
            if self.practice_only() {
                ui::chip(ui, "Simulator: practice only, not saved", theme::WARN);
            }
            ui.label(ui::small(format!(
                "Score {UNLOCK_SCORE_PERCENT:.0}% or more to unlock the next level."
            )));
        });
        ui.add_space(6.0);
        if !self.tracker.is_calibrated() {
            card(ui, |ui| {
                ui.label(
                    RichText::new("Calibrate first so your hand can drive the blade").strong(),
                );
                self.calibration_panel(ui);
            });
            ui.add_space(8.0);
        }
        let mut start: Option<GameLevel> = None;
        ui.columns(status.len(), |cols| {
            for ((level, unlocked, best), ui) in status.iter().zip(cols.iter_mut()) {
                let tint = if *unlocked {
                    theme::ACCENT
                } else {
                    theme::FAINT
                };
                ui::tinted_card(ui, tint, |ui| {
                    ui.set_min_height(250.0);
                    ui.label(
                        RichText::new(match level {
                            GameLevel::AppleTrace => "🍎",
                            GameLevel::PnfD1 => "🍉",
                            GameLevel::PnfD2 => "⚔",
                        })
                        .size(34.0),
                    );
                    ui.label(RichText::new(level.title()).size(19.0).strong());
                    ui.label(ui::muted(level.description()));
                    ui.label(ui::small(format!(
                        "Trains: {}",
                        level
                            .muscles()
                            .iter()
                            .map(|m| m.label())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                    ui.horizontal(|ui| {
                        ui::stars(ui, best.map_or(0, crate::game::stars_for), 20.0);
                        ui.label(ui::small(
                            best.map_or("Not played yet".into(), |b| format!("Best {b:.0}%")),
                        ));
                    });
                    if *unlocked {
                        if ui::primary(ui, "Play").clicked() {
                            start = Some(*level);
                        }
                    } else {
                        let prev = level.previous().map_or("", |p| p.short());
                        ui.label(
                            RichText::new(format!(
                                "🔒 Score {UNLOCK_SCORE_PERCENT:.0}% on {prev} to unlock"
                            ))
                            .color(theme::WARN),
                        );
                    }
                });
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Reps per PNF round:");
            for n in [5u32, 8, 10] {
                if ui
                    .selectable_label(self.store.settings.reps_per_round == n, n.to_string())
                    .clicked()
                {
                    self.store.settings.reps_per_round = n;
                    self.store.save_cache();
                }
            }
            ui.separator();
            ui.checkbox(&mut self.play.bombs, "Bombs off the diagonal");
            ui.separator();
            let mut unlock = self.store.settings.physio_unlock_all;
            if ui
                .checkbox(&mut unlock, "Physio override: unlock all levels")
                .changed()
            {
                self.store.settings.physio_unlock_all = unlock;
                self.store.save_cache();
            }
        });
        if let Some(level) = start {
            self.start_game(level);
        }
    }

    pub(crate) fn start_game(&mut self, level: GameLevel) {
        let Some(patient) = self.current_patient() else {
            return;
        };
        let rom = self
            .store
            .records_for(&patient.id)
            .iter()
            .filter(|r| r.movement == Movement::ShoulderAbduction)
            .map(|r| r.peak_angle_degrees.unwrap_or(r.angle_degrees))
            .fold(None, |m: Option<f64>, a| Some(m.map_or(a, |m| m.max(a))));
        self.play.level = level;
        self.play.result = None;
        self.play.unlocked = None;
        self.play.paused = false;
        self.play.fx = Fx::default();
        let mut game = Game::new(
            level,
            self.side(),
            rom,
            self.store.settings.reps_per_round,
            self.play.bombs,
            fastrand::u64(..),
        );
        game.practice = self.practice_only();
        self.play.game = Some(game);
        self.voice.say(
            match level {
                GameLevel::AppleTrace => "Let's draw an apple! Start at the glowing circle.",
                GameLevel::PnfD1 => "Level one. Slice from your hip up across to the opposite ear.",
                GameLevel::PnfD2 => {
                    "Level two. Draw your sword: slice from the opposite hip up and out."
                }
            },
            true,
        );
    }

    fn game_canvas(&mut self, ui: &mut egui::Ui) {
        let side = self.side();
        let mut action: Option<&str> = None;
        ui.horizontal(|ui| {
            let game = self.play.game.as_ref().expect("game");
            ui.label(RichText::new(game.level.title()).size(20.0).strong());
            ui.label(ui::muted(format!("{} arm", side.label())));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Quit").clicked() {
                    action = Some("quit");
                }
                if game.stage != Stage::Finished {
                    let skip_label = if game.level == GameLevel::AppleTrace {
                        "Skip apple"
                    } else {
                        "End round"
                    };
                    if ui.button(skip_label).clicked() {
                        action = Some("skip");
                    }
                    if ui
                        .button(if self.play.paused { "Resume" } else { "Pause" })
                        .clicked()
                    {
                        action = Some("pause");
                    }
                }
                if !self.tracker.is_calibrated() {
                    ui.label(RichText::new("Not calibrated").color(theme::WARN));
                }
            });
        });
        match action {
            Some("quit") => {
                self.play.game = None;
                return;
            }
            Some("skip") => {
                let events = self.play.game.as_mut().map(Game::skip).unwrap_or_default();
                self.handle_game_events(&events);
            }
            Some("pause") => self.play.paused = !self.play.paused,
            _ => {}
        }

        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
        let rect = response.rect;
        let view = View::new(rect, side, self.play.fx.shake, self.play.fx.time);
        if self.simulated() {
            match response.hover_pos() {
                Some(pos) if self.demo.is_none() => {
                    self.sim.input = SimInput::HandTarget(view.game(pos))
                }
                _ => {}
            }
        }
        let t = self.play.fx.time;
        let pulse = ((t * 4.0).sin() * 0.5 + 0.5) as f32;
        wood_background(&painter, rect);
        for s in &self.play.fx.splats {
            painter.circle_filled(
                view.pos(s.pos),
                view.len(s.radius),
                rgba(s.color, (s.life / 3.0) as f32 * 0.3),
            );
        }
        ghost_body(&painter, &view, side);
        if let Some(pose) = self.tracker.pose {
            let arm = Color32::from_rgba_unmultiplied(255, 255, 255, 60);
            painter.line_segment(
                [view.pos([0.0, 0.0]), view.pos(pose.elbow)],
                Stroke::new(view.len(0.08), arm),
            );
            painter.line_segment(
                [view.pos(pose.elbow), view.pos(pose.hand)],
                Stroke::new(view.len(0.07), arm),
            );
            painter.circle_filled(view.pos([0.0, 0.0]), view.len(0.05), arm);
        }

        let game = self.play.game.as_ref().expect("game");
        let instruction: String;
        match &game.mode {
            Mode::Apple(round) => {
                let tracer = &round.tracer;
                let pts: Vec<Pos2> = tracer.path.points.iter().map(|p| view.pos(*p)).collect();
                if round.pause <= 0.0 {
                    let fill: Vec<Pos2> = pts.clone();
                    painter.add(Shape::convex_polygon(
                        fill,
                        Color32::from_rgba_unmultiplied(214, 40, 40, 34),
                        Stroke::NONE,
                    ));
                    let traced = tracer.next.min(pts.len());
                    if traced < pts.len() {
                        painter.add(Shape::dashed_line(
                            &pts[traced.saturating_sub(1)..],
                            Stroke::new(4.0, Color32::from_white_alpha(170)),
                            10.0,
                            8.0,
                        ));
                    }
                    if traced >= 2 {
                        painter.add(Shape::line(
                            pts[..traced].to_vec(),
                            Stroke::new(16.0, Color32::from_rgba_unmultiplied(255, 82, 82, 70)),
                        ));
                        painter.add(Shape::line(
                            pts[..traced].to_vec(),
                            Stroke::new(7.0, Color32::from_rgb(0xFF, 0x52, 0x52)),
                        ));
                    }
                    let top = view.pos([
                        tracer.path.center[0],
                        tracer.path.center[1] + tracer.path.radius * 0.78,
                    ]);
                    let r = view.len(tracer.path.radius);
                    painter.line_segment(
                        [top, top + Vec2::new(r * 0.08, -r * 0.35)],
                        Stroke::new(r * 0.07, Color32::from_rgb(0x5D, 0x3A, 0x1A)),
                    );
                    painter.add(Shape::ellipse_filled(
                        top + Vec2::new(r * 0.32, -r * 0.3),
                        Vec2::new(r * 0.24, r * 0.1),
                        Color32::from_rgb(0x4C, 0xAF, 0x50),
                    ));
                    if tracer.started {
                        let guide = (tracer.next + 4 + ((t * 6.0) as usize % 6)).min(pts.len() - 1);
                        painter.circle_filled(pts[guide], 7.0, Color32::from_white_alpha(200));
                        if tracer.next < pts.len() {
                            painter.circle_stroke(
                                pts[tracer.next],
                                12.0 + 4.0 * pulse,
                                Stroke::new(2.5, theme::ACCENT),
                            );
                        }
                        instruction = format!(
                            "Follow the outline · {:.0}% traced",
                            tracer.coverage() * 100.0
                        );
                    } else {
                        let s = pts[0];
                        painter.circle_filled(
                            s,
                            view.len(tracer.tolerance * 1.6) * (1.0 + 0.15 * pulse),
                            theme::ACCENT.gamma_multiply(0.3),
                        );
                        painter.circle_stroke(
                            s,
                            view.len(tracer.tolerance * 1.6),
                            Stroke::new(3.0, theme::ACCENT),
                        );
                        painter.text(
                            s + Vec2::new(0.0, -view.len(tracer.tolerance * 1.6) - 14.0),
                            Align2::CENTER_CENTER,
                            "START",
                            FontId::proportional(16.0),
                            theme::ACCENT,
                        );
                        instruction = "Move your hand to the glowing START circle".into();
                    }
                } else {
                    instruction = "Sliced! Next apple coming…".into();
                }
            }
            Mode::Pnf(round) => {
                let path = round.path;
                let guide: Vec<Pos2> = (0..=40)
                    .map(|i| view.pos(path.point(i as f64 / 40.0)))
                    .collect();
                painter.add(Shape::dashed_line(
                    &guide,
                    Stroke::new(3.0, Color32::from_white_alpha(110)),
                    12.0,
                    10.0,
                ));
                for i in [0.3, 0.55, 0.8] {
                    let a = view.pos(path.point(i));
                    let tan = path.tangent(i);
                    let dir = Vec2::new(tan[0] as f32, -tan[1] as f32);
                    let n = Vec2::new(-dir.y, dir.x);
                    painter.add(Shape::convex_polygon(
                        vec![
                            a + dir * 12.0,
                            a - dir * 6.0 + n * 8.0,
                            a - dir * 6.0 - n * 8.0,
                        ],
                        Color32::from_white_alpha(120),
                        Stroke::NONE,
                    ));
                }
                let (start_color, end_color) = match round.phase {
                    Phase::GoToStart | Phase::Return => {
                        (theme::ACCENT, Color32::from_white_alpha(120))
                    }
                    _ => (
                        Color32::from_white_alpha(120),
                        Color32::from_rgb(0xFF, 0xD5, 0x4F),
                    ),
                };
                zone(
                    &painter,
                    &view,
                    path.start,
                    "Basket",
                    start_color,
                    pulse,
                    ZoneKind::Basket,
                );
                zone(
                    &painter,
                    &view,
                    path.end,
                    "Top",
                    end_color,
                    pulse,
                    ZoneKind::Target,
                );
                for f in round.fruits.iter().filter(|f| !f.sliced) {
                    draw_fruit(&painter, &view, f.pos, f.kind, f.radius, t + f.spin);
                }
                for b in round.bombs.iter().filter(|b| !b.hit) {
                    draw_bomb(&painter, &view, b.pos, b.radius, t);
                }
                instruction = match round.phase {
                    Phase::GoToStart => format!(
                        "Rep {}/{} · hand to the basket to start",
                        (round.rep + 1).min(round.reps_total),
                        round.reps_total
                    ),
                    Phase::Swipe => format!(
                        "Rep {}/{} · slice up along the diagonal to the star",
                        round.rep, round.reps_total
                    ),
                    Phase::Return => "Bring it back to the basket".into(),
                    Phase::Done => String::new(),
                };
            }
        }
        draw_fx(&painter, &view, &self.play.fx);
        draw_trail(&painter, &view, &self.play.fx);
        if let Some(hand) = self.tracker.hand {
            let c = view.pos(hand);
            painter.circle_filled(c, 16.0, theme::ACCENT.gamma_multiply(0.35));
            painter.circle_filled(c, 8.0, Color32::WHITE);
        }

        // HUD
        let hud = |text: String, at: Pos2, align: Align2, size: f32, color: Color32| {
            painter.text(
                at + Vec2::new(2.0, 2.0),
                align,
                &text,
                FontId::proportional(size),
                Color32::from_black_alpha(150),
            );
            painter.text(at, align, text, FontId::proportional(size), color);
        };
        let points = game.points();
        hud(
            format!("{points}"),
            rect.right_top() + Vec2::new(-24.0, 18.0),
            Align2::RIGHT_TOP,
            40.0,
            Color32::from_rgb(0xFF, 0xD5, 0x4F),
        );
        hud(
            "points".into(),
            rect.right_top() + Vec2::new(-24.0, 62.0),
            Align2::RIGHT_TOP,
            15.0,
            theme::TEXT,
        );
        let progress = match &game.mode {
            Mode::Apple(r) => format!(
                "Apple {}/{}",
                (r.done + 1).min(crate::game::APPLES_PER_ROUND),
                crate::game::APPLES_PER_ROUND
            ),
            Mode::Pnf(r) => format!("Combo x{}  ·  Bombs {}", r.combo, r.bombs_hit),
        };
        hud(
            progress,
            rect.left_top() + Vec2::new(22.0, 18.0),
            Align2::LEFT_TOP,
            22.0,
            theme::TEXT,
        );
        hud(
            format!("Score {:.0}%", game.score_percent()),
            rect.left_top() + Vec2::new(22.0, 48.0),
            Align2::LEFT_TOP,
            16.0,
            theme::MUTED,
        );
        if let Some(p) = self.tracker.pose {
            hud(
                format!(
                    "Shoulder {:.0}°  ·  best {:.0}°",
                    p.elevation_deg, game.max_elevation
                ),
                rect.left_bottom() + Vec2::new(22.0, -22.0),
                Align2::LEFT_BOTTOM,
                16.0,
                theme::TEXT,
            );
        }
        if !instruction.is_empty() && game.stage == Stage::Playing {
            hud(
                instruction,
                Pos2::new(rect.center().x, rect.bottom() - 26.0),
                Align2::CENTER_BOTTOM,
                20.0,
                Color32::WHITE,
            );
        }
        if let Stage::Countdown(left) = game.stage {
            painter.rect_filled(rect, CornerRadius::same(14), Color32::from_black_alpha(90));
            hud(
                format!("{}", left.ceil() as u32),
                rect.center(),
                Align2::CENTER_CENTER,
                120.0,
                Color32::WHITE,
            );
            hud(
                "Get ready".into(),
                rect.center() + Vec2::new(0.0, 80.0),
                Align2::CENTER_CENTER,
                22.0,
                theme::TEXT,
            );
        }
        if self.play.paused {
            painter.rect_filled(rect, CornerRadius::same(14), Color32::from_black_alpha(140));
            hud(
                "Paused".into(),
                rect.center(),
                Align2::CENTER_CENTER,
                56.0,
                Color32::WHITE,
            );
        }
        if !self.tracker.has_live_data() {
            hud(
                "Waiting for sensor data… (game paused)".into(),
                rect.center() - Vec2::new(0.0, 120.0),
                Align2::CENTER_CENTER,
                22.0,
                theme::WARN,
            );
        }
        let finished = game.stage == Stage::Finished;
        if finished {
            self.result_overlay(ui, rect);
        }
    }

    fn result_overlay(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let Some(result) = self.play.result.clone() else {
            return;
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(14), Color32::from_black_alpha(150));
        let panel = Rect::from_center_size(
            rect.center(),
            Vec2::new(520.0, 380.0).min(rect.size() - Vec2::splat(20.0)),
        );
        let mut action = None;
        let saved = self.store.games.iter().any(|g| g.id == result.id);
        ui.scope_builder(egui::UiBuilder::new().max_rect(panel), |ui| {
            card(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(result.level.title()).size(18.0).color(theme::MUTED));
                    ui.label(RichText::new(format!("{:.0}%", result.score_percent)).size(64.0).strong().color(theme::ACCENT));
                    ui::stars(ui, result.stars, 36.0);
                    ui.label(ui::muted(format!(
                        "{} points · {} reps ({} clean) · accuracy {:.0}% · shoulder up to {:.0}°{}",
                        result.points,
                        result.reps,
                        result.clean_reps,
                        result.accuracy_percent,
                        result.max_elevation_deg,
                        if result.bombs_hit > 0 { format!(" · {} bomb(s)", result.bombs_hit) } else { String::new() }
                    )));
                    if !saved {
                        ui.label(RichText::new("Simulator round, not saved").color(theme::WARN));
                    } else if let Some(level) = self.play.unlocked {
                        ui.label(RichText::new(format!("🔓 New level unlocked: {}", level.title())).size(18.0).color(theme::GOOD));
                    } else if result.score_percent < UNLOCK_SCORE_PERCENT && result.level.next().is_some() {
                        ui.label(RichText::new(format!("Score {UNLOCK_SCORE_PERCENT:.0}% to unlock the next level - keep going!")).color(theme::WARN));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui::primary(ui, "Play again").clicked() {
                            action = Some(result.level);
                        }
                        if let Some(next) = self.play.unlocked.or_else(|| {
                            result.level.next().filter(|n| {
                                let games = self.store.games_for(&result.patient_id);
                                is_unlocked(&games, *n, self.store.settings.physio_unlock_all)
                            })
                        }) && ui::secondary(ui, &format!("Next: {}", next.short())).clicked()
                        {
                            action = Some(next);
                        }
                        if ui::secondary(ui, "Levels").clicked() {
                            self.play.game = None;
                        }
                    });
                });
            });
        });
        if let Some(level) = action {
            self.start_game(level);
        }
    }
}

/// Visual, audio and persistence side effects of game events.
pub fn apply_event(app: &mut KinetismApp, event: GameEvent) {
    let level = app.play.level;
    let cut_angle = {
        let t = &app.play.fx.trail;
        match (t.len().checked_sub(2).and_then(|i| t.get(i)), t.back()) {
            (Some((a, _)), Some((b, _))) => (b[1] - a[1]).atan2(b[0] - a[0]),
            _ => 0.6,
        }
    };
    let hand = app.tracker.hand.unwrap_or([0.0, 0.0]);
    match event {
        GameEvent::Go => app.voice.say("Go!", true),
        GameEvent::TraceStarted => app.voice.say("Great start! Follow the outline.", false),
        GameEvent::TraceMilestone(m) => {
            app.play.fx.popup(
                [hand[0], hand[1] + 0.12],
                format!("{m}%"),
                [255, 255, 255],
                false,
            );
            if m == 50 {
                app.voice.say("Halfway there!", false);
            }
        }
        GameEvent::AppleSliced {
            center,
            radius,
            score,
        } => {
            app.play
                .fx
                .slice(center, FruitKind::Apple, radius, cut_angle);
            app.play.fx.popup(
                center,
                format!("+{}", (score * 100.0).round()),
                [255, 213, 79],
                true,
            );
            app.voice.say(
                if score >= 0.9 {
                    "Perfect apple!"
                } else {
                    "Apple sliced!"
                },
                true,
            );
        }
        GameEvent::RepStarted(n) => {
            if n == 1 {
                app.voice.say("Slice the fruit!", false);
            }
        }
        GameEvent::FruitSliced { pos, kind, clean } => {
            app.play.fx.slice(pos, kind, 0.075, cut_angle);
            app.play.fx.popup(
                [pos[0], pos[1] + 0.1],
                if clean { "+15" } else { "+8" },
                if clean {
                    [255, 255, 255]
                } else {
                    [200, 200, 200]
                },
                false,
            );
        }
        GameEvent::BombHit(pos) => {
            app.play.fx.bomb(pos);
            app.play
                .fx
                .popup([pos[0], pos[1] + 0.12], "BOOM -20", [255, 120, 80], true);
            app.voice.say("Careful! Stay on the diagonal.", true);
        }
        GameEvent::ReachedTop => {
            app.play
                .fx
                .popup([hand[0], hand[1] + 0.1], "TOP!", [255, 213, 79], false);
        }
        GameEvent::RepDone { clean, combo } => {
            if combo >= 2 {
                app.play.fx.popup(
                    [hand[0], hand[1] + 0.25],
                    format!("COMBO x{combo}!"),
                    [255, 213, 79],
                    true,
                );
                app.voice.say(&format!("Combo {combo}!"), false);
            } else if !clean {
                app.voice.say("Good effort. Follow the dotted line.", false);
            }
        }
        GameEvent::Finished { score, stars } => {
            let Some(patient) = app.current_patient() else {
                return;
            };
            let Some(game) = app.play.game.as_ref() else {
                return;
            };
            let session = game.to_session(&patient, chrono::Utc::now());
            if game.practice || app.practice_only() {
                // Shown, but neither saved nor unlocking: the simulator is not the patient.
                app.play.unlocked = None;
                app.play.result = Some(session);
                app.voice
                    .say(&format!("Practice round: {score:.0} percent."), true);
                return;
            }
            let next = level.next();
            let override_all = app.store.settings.physio_unlock_all;
            let was_unlocked = next
                .is_none_or(|n| is_unlocked(&app.store.games_for(&patient.id), n, override_all));
            app.store.add_game(session.clone());
            let now_unlocked = next
                .is_some_and(|n| is_unlocked(&app.store.games_for(&patient.id), n, override_all));
            app.play.unlocked = (!was_unlocked && now_unlocked).then_some(next).flatten();
            app.play.result = Some(session);
            let line = match (app.play.unlocked.is_some(), stars) {
                (true, _) => format!("Amazing! {score:.0} percent. New level unlocked!"),
                (_, 3) => format!("Brilliant! {score:.0} percent, three stars!"),
                (_, 2) => format!("Well done! {score:.0} percent."),
                _ => format!("Good work! {score:.0} percent. Let's try again."),
            };
            app.voice.say(&line, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::dist;

    fn canvas(width: f32, height: f32) -> Rect {
        Rect::from_min_size(Pos2::new(40.0, 25.0), Vec2::new(width, height))
    }

    #[test]
    fn view_round_trips_points_and_shows_the_full_reach_for_both_sides() {
        for rect in [canvas(1100.0, 640.0), canvas(560.0, 900.0)] {
            for side in [Side::Left, Side::Right] {
                let view = View::new(rect, side, 0.0, 0.0);
                let shoulder = view.pos([0.0, 0.0]);
                assert!(
                    (shoulder.x - rect.center().x) * (side.sign() as f32) < 0.0,
                    "{side:?}: the shoulder is drawn toward the other side to leave room for the arm"
                );
                assert!(
                    view.pos([0.3, 0.0]).x > shoulder.x,
                    "{side:?}: game x grows to the right on screen (mirror view)"
                );
                assert!(view.pos([0.0, 0.3]).y < shoulder.y, "{side:?}: y is up");
                for p in [[0.0, 0.0], [0.62, -0.35], [-0.9, 0.8], [1.0, 0.0]] {
                    let back = view.game(view.pos(p));
                    assert!(
                        dist(back, p) < 1e-4,
                        "{side:?}: {p:?} came back as {back:?}"
                    );
                }
                for i in 0..72 {
                    let a = f64::from(i) * std::f64::consts::TAU / 72.0;
                    let hand = view.pos([a.cos(), a.sin()]);
                    assert!(
                        rect.expand(0.5).contains(hand),
                        "{side:?}: a fully reaching hand at {:.0} deg is drawn at {hand:?}, outside {rect:?}",
                        a.to_degrees()
                    );
                }
            }
        }
    }

    #[test]
    fn screen_shake_moves_the_picture_but_not_the_pointer_target() {
        let rect = canvas(1100.0, 640.0);
        let calm = View::new(rect, Side::Right, 0.0, 1.3);
        let shaken = View::new(rect, Side::Right, 1.0, 1.3);
        let jolt = shaken.pos([0.4, 0.1]) - calm.pos([0.4, 0.1]);
        assert!(
            jolt.length() > 1.0 && jolt.x.abs() <= 9.0 && jolt.y.abs() <= 9.0,
            "a full shake moves the drawing by up to 9 px: {jolt:?}"
        );
        let pointer = Pos2::new(700.0, 300.0);
        assert_eq!(
            shaken.game(pointer),
            calm.game(pointer),
            "the simulator's mouse target stays put while the screen shakes"
        );
    }
}
