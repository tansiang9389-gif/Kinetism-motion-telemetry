//! Stick figures, calibration illustrations and the muscle body map.

use crate::clinical::{Movement, Muscle, Side};
use crate::kinematics::ArmPose;
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2,
};
use std::collections::BTreeMap;

fn bone(painter: &egui::Painter, a: Pos2, b: Pos2, width: f32, color: Color32) {
    painter.line_segment([a, b], Stroke::new(width, color));
    painter.circle_filled(a, width * 0.55, color);
    painter.circle_filled(b, width * 0.55, color);
}

/// Front (mirror) view of the patient with the measured arm from the live pose.
pub fn measure_figure(
    ui: &mut Ui,
    size: Vec2,
    pose: Option<&ArmPose>,
    side: Side,
    movement: Movement,
) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(12), theme::DEEP);
    let unit = rect.height().min(rect.width() * 0.9) * 0.36;
    let s = side.sign() as f32;
    let shoulder = Pos2::new(
        rect.center().x + s * unit * 0.32,
        rect.top() + rect.height() * 0.34,
    );
    let other_shoulder = Pos2::new(rect.center().x - s * unit * 0.32, shoulder.y);
    let neck = Pos2::new(rect.center().x, shoulder.y - unit * 0.08);
    let hip_y = shoulder.y + unit * 1.15;
    let body = Color32::from_rgb(0x3D, 0x55, 0x62);
    painter.circle_filled(Pos2::new(neck.x, neck.y - unit * 0.28), unit * 0.2, body);
    painter.add(Shape::convex_polygon(
        vec![
            Pos2::new(other_shoulder.x, other_shoulder.y),
            Pos2::new(shoulder.x, shoulder.y),
            Pos2::new(rect.center().x + s * unit * 0.24, hip_y),
            Pos2::new(rect.center().x - s * unit * 0.24, hip_y),
        ],
        body,
        Stroke::NONE,
    ));
    let rest_arm = |from: Pos2| {
        [
            from,
            Pos2::new(from.x, from.y + unit * 0.55),
            Pos2::new(from.x, from.y + unit),
        ]
    };
    let other = rest_arm(other_shoulder);
    bone(&painter, other[0], other[1], unit * 0.11, body);
    bone(&painter, other[1], other[2], unit * 0.1, body);

    let to_screen = |p: [f64; 2]| {
        Pos2::new(
            shoulder.x + p[0] as f32 * unit,
            shoulder.y - p[1] as f32 * unit,
        )
    };
    let (elbow, hand) = match pose {
        Some(p) => (to_screen(p.elbow), to_screen(p.hand)),
        None => {
            let r = rest_arm(shoulder);
            (r[1], r[2])
        }
    };
    if movement == Movement::ShoulderAbduction
        && let Some(p) = pose
    {
        // Arc from hanging straight down towards the lateral side (screen y is down).
        let radius = unit * 0.45;
        let sweep = (p.elevation_deg as f32).to_radians();
        let pts: Vec<Pos2> = (0..=32)
            .map(|i| {
                let a = sweep * i as f32 / 32.0;
                Pos2::new(
                    shoulder.x + s * a.sin() * radius,
                    shoulder.y + a.cos() * radius,
                )
            })
            .collect();
        painter.add(Shape::line(pts, Stroke::new(3.0, theme::ACCENT)));
        let mid = sweep * 0.5;
        painter.text(
            Pos2::new(
                shoulder.x + s * mid.sin() * radius * 1.35,
                shoulder.y + mid.cos() * radius * 1.35,
            ),
            Align2::CENTER_CENTER,
            format!("{:.0}°", p.elevation_deg),
            FontId::proportional(18.0),
            theme::ACCENT,
        );
    }
    bone(&painter, shoulder, elbow, unit * 0.12, theme::ACCENT);
    bone(
        &painter,
        elbow,
        hand,
        unit * 0.11,
        Color32::from_rgb(0x8F, 0xEF, 0xDC),
    );
    painter.circle_filled(hand, unit * 0.075, Color32::WHITE);
    painter.text(
        Pos2::new(rect.right() - 10.0, rect.top() + 10.0),
        Align2::RIGHT_TOP,
        format!("{} arm · mirror view", side.label()),
        FontId::proportional(13.0),
        theme::FAINT,
    );
}

/// Calibration pose illustration: 1 = arm relaxed at side, 2 = arm forward.
pub fn calibration_pose(ui: &mut Ui, size: Vec2, step: u8, active: bool) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(12), theme::DEEP);
    let color = if active { theme::ACCENT } else { theme::FAINT };
    let body = Color32::from_rgb(0x3D, 0x55, 0x62);
    let u = rect.height() * 0.22;
    let c = Pos2::new(rect.center().x - u * 0.3, rect.top() + rect.height() * 0.3);
    // Side view: head, trunk, chair.
    painter.circle_filled(Pos2::new(c.x, c.y - u * 0.55), u * 0.32, body);
    bone(&painter, c, Pos2::new(c.x, c.y + u * 1.6), u * 0.28, body);
    bone(
        &painter,
        Pos2::new(c.x, c.y + u * 1.6),
        Pos2::new(c.x + u * 1.0, c.y + u * 1.6),
        u * 0.22,
        body,
    );
    bone(
        &painter,
        Pos2::new(c.x + u * 1.0, c.y + u * 1.6),
        Pos2::new(c.x + u * 1.0, c.y + u * 2.5),
        u * 0.2,
        body,
    );
    let (elbow, hand) = if step == 1 {
        (
            Pos2::new(c.x, c.y + u * 0.95),
            Pos2::new(c.x, c.y + u * 1.8),
        )
    } else {
        let a = 30f32.to_radians();
        let dir = Vec2::new(a.cos(), a.sin());
        (c + dir * u * 0.95, c + dir * u * 1.85)
    };
    bone(&painter, c, elbow, u * 0.22, color);
    bone(&painter, elbow, hand, u * 0.2, color);
    painter.text(
        Pos2::new(rect.left() + 10.0, rect.top() + 8.0),
        Align2::LEFT_TOP,
        if step == 1 {
            "1  Arm relaxed at side"
        } else {
            "2  Arm forward, elbow straight"
        },
        FontId::proportional(13.0),
        color,
    );
}

struct MuscleShape {
    muscle: Muscle,
    front: bool,
    center: [f32; 2],
    radius: [f32; 2],
}

/// Authored on the viewer's left half of each view; mirrored for the other side.
const SHAPES: [MuscleShape; 7] = [
    MuscleShape {
        muscle: Muscle::MiddleDeltoid,
        front: true,
        center: [0.255, 0.285],
        radius: [0.034, 0.048],
    },
    MuscleShape {
        muscle: Muscle::AnteriorDeltoid,
        front: true,
        center: [0.305, 0.300],
        radius: [0.030, 0.045],
    },
    MuscleShape {
        muscle: Muscle::PectoralisMajor,
        front: true,
        center: [0.405, 0.335],
        radius: [0.075, 0.048],
    },
    MuscleShape {
        muscle: Muscle::BicepsBrachii,
        front: true,
        center: [0.262, 0.395],
        radius: [0.026, 0.07],
    },
    MuscleShape {
        muscle: Muscle::Supraspinatus,
        front: false,
        center: [0.375, 0.272],
        radius: [0.06, 0.02],
    },
    MuscleShape {
        muscle: Muscle::Infraspinatus,
        front: false,
        center: [0.365, 0.335],
        radius: [0.062, 0.042],
    },
    MuscleShape {
        muscle: Muscle::MiddleDeltoid,
        front: false,
        center: [0.255, 0.285],
        radius: [0.034, 0.048],
    },
];

fn silhouette(painter: &egui::Painter, to: &dyn Fn(f32, f32) -> Pos2, scale: f32, color: Color32) {
    painter.circle_filled(to(0.5, 0.12), 0.07 * scale, color);
    painter.add(Shape::convex_polygon(
        vec![
            to(0.30, 0.245),
            to(0.70, 0.245),
            to(0.63, 0.62),
            to(0.37, 0.62),
        ],
        color,
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![
            to(0.46, 0.17),
            to(0.54, 0.17),
            to(0.55, 0.25),
            to(0.45, 0.25),
        ],
        color,
        Stroke::NONE,
    ));
    for (sx, ex, wx) in [(0.29, 0.245, 0.225), (0.71, 0.755, 0.775)] {
        bone(painter, to(sx, 0.27), to(ex, 0.45), 0.055 * scale, color);
        bone(painter, to(ex, 0.45), to(wx, 0.62), 0.045 * scale, color);
    }
    painter.add(Shape::convex_polygon(
        vec![
            to(0.37, 0.62),
            to(0.63, 0.62),
            to(0.66, 0.74),
            to(0.34, 0.74),
        ],
        color,
        Stroke::NONE,
    ));
}

/// Front + back body map. Returns the muscle under the pointer or clicked.
pub fn body_map(
    ui: &mut Ui,
    size: Vec2,
    grades: &BTreeMap<Muscle, Option<u8>>,
    side: Side,
    selected: Option<Muscle>,
) -> (Option<Muscle>, Option<Muscle>) {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(12), theme::DEEP);
    let half = rect.width() / 2.0;
    let mut hovered = None;
    let pointer = response.hover_pos();
    for front in [true, false] {
        let panel = Rect::from_min_size(
            Pos2::new(rect.left() + if front { 0.0 } else { half }, rect.top()),
            Vec2::new(half, rect.height()),
        );
        let scale = panel.height().min(panel.width() * 1.25);
        let origin = Pos2::new(
            panel.center().x - scale * 0.5,
            panel.top() + (panel.height() - scale) * 0.5 + 6.0,
        );
        let to = |x: f32, y: f32| Pos2::new(origin.x + x * scale, origin.y + y * scale);
        silhouette(&painter, &to, scale, Color32::from_rgb(0x2B, 0x3E, 0x49));
        painter.text(
            Pos2::new(panel.center().x, panel.bottom() - 14.0),
            Align2::CENTER_CENTER,
            if front { "FRONT" } else { "BACK" },
            FontId::proportional(12.0),
            theme::FAINT,
        );
        // Front view: patient's right is on the viewer's left. Back view: on the right.
        let (l, r) = if front { ("R", "L") } else { ("L", "R") };
        painter.text(
            to(0.2, 0.2),
            Align2::CENTER_CENTER,
            l,
            FontId::proportional(14.0),
            theme::FAINT,
        );
        painter.text(
            to(0.8, 0.2),
            Align2::CENTER_CENTER,
            r,
            FontId::proportional(14.0),
            theme::FAINT,
        );
        let affected_on_left = (side == Side::Right) == front;
        for shape in SHAPES.iter().filter(|s| s.front == front) {
            for on_left in [true, false] {
                let cx = if on_left {
                    shape.center[0]
                } else {
                    1.0 - shape.center[0]
                };
                let is_affected = on_left == affected_on_left;
                let center = to(cx, shape.center[1]);
                let radius = Vec2::new(shape.radius[0] * scale, shape.radius[1] * scale);
                let grade = grades.get(&shape.muscle).copied().flatten();
                let game_only = shape.muscle.graded_by().is_none();
                let fill = if !is_affected {
                    Color32::from_rgb(0x33, 0x47, 0x52)
                } else if let Some(g) = grade {
                    theme::grade_color(g)
                } else if game_only {
                    theme::ACCENT.gamma_multiply(0.35)
                } else {
                    theme::NO_DATA
                };
                painter.add(Shape::ellipse_filled(center, radius, fill));
                if is_affected && grade.is_none() {
                    let outline = if game_only {
                        theme::ACCENT
                    } else {
                        theme::FAINT
                    };
                    painter.add(Shape::ellipse_stroke(
                        center,
                        radius,
                        Stroke::new(1.0, outline),
                    ));
                }
                let is_selected = selected == Some(shape.muscle) && is_affected;
                if is_selected {
                    painter.add(Shape::ellipse_stroke(
                        center,
                        radius + Vec2::splat(3.0),
                        Stroke::new(2.0, Color32::WHITE),
                    ));
                }
                if is_affected && let Some(p) = pointer {
                    let d = Vec2::new((p.x - center.x) / radius.x, (p.y - center.y) / radius.y);
                    if d.length_sq() <= 1.2 {
                        hovered = Some(shape.muscle);
                        painter.add(Shape::ellipse_stroke(
                            center,
                            radius + Vec2::splat(2.0),
                            Stroke::new(1.5, theme::ACCENT),
                        ));
                    }
                }
            }
        }
    }
    let clicked = if response.clicked() { hovered } else { None };
    if let Some(m) = hovered {
        let grade = grades.get(&m).copied().flatten();
        response.on_hover_text(match grade {
            Some(g) => format!(
                "{} - MMT {g}/5 ({})",
                m.label(),
                crate::clinical::mmt_title(g)
            ),
            None if m.graded_by().is_some() => format!("{} - not measured yet", m.label()),
            None => format!("{} - trained in the PNF game (no MMT test)", m.label()),
        });
    }
    (hovered, clicked)
}
