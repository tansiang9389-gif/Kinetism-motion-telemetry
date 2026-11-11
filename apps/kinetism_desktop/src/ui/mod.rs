//! Shared widgets and drawings.

pub mod figures;
pub mod game_view;
pub mod hub;
pub mod measure;
pub mod patients;
pub mod progress;
pub mod records;

use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Margin, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0, theme::SURFACE_HI))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| ui.vertical(add).inner)
        .inner
}

pub fn tinted_card<R>(ui: &mut Ui, tint: Color32, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(tint.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, tint.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| ui.vertical(add).inner)
        .inner
}

/// Five-point star built from convex pieces (the default font has no ★ glyph).
pub fn paint_star(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    let point = |i: usize, r: f32| {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
        center + Vec2::new(a.cos(), a.sin()) * r
    };
    let inner: Vec<Pos2> = (0..5).map(|k| point(2 * k + 1, radius * 0.45)).collect();
    painter.add(egui::Shape::convex_polygon(
        inner.clone(),
        color,
        Stroke::NONE,
    ));
    for k in 0..5 {
        painter.add(egui::Shape::convex_polygon(
            vec![inner[(k + 4) % 5], point(2 * k, radius), inner[k]],
            color,
            Stroke::NONE,
        ));
    }
}

pub fn eyebrow(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(12.0)
            .strong()
            .color(theme::ACCENT),
    );
}

pub fn title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(26.0).strong());
}

pub fn subtitle(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).color(theme::MUTED));
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(theme::MUTED)
}

pub fn small(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).size(13.0).color(theme::MUTED)
}

pub fn primary(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).strong().color(theme::BG))
            .fill(theme::ACCENT)
            .min_size(Vec2::new(0.0, 40.0)),
    )
}

pub fn primary_enabled(ui: &mut Ui, enabled: bool, text: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).strong().color(theme::BG))
            .fill(theme::ACCENT)
            .min_size(Vec2::new(0.0, 40.0)),
    )
}

pub fn secondary(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(text).min_size(Vec2::new(0.0, 40.0)))
}

pub fn metric(ui: &mut Ui, label: &str, value: &str, color: Color32) {
    ui.vertical(|ui| {
        ui.label(small(label));
        ui.label(RichText::new(value).size(28.0).strong().color(color));
    });
}

pub fn chip(ui: &mut Ui, text: &str, color: Color32) -> egui::Response {
    let galley_font = FontId::proportional(13.0);
    let text_size = ui
        .painter()
        .layout_no_wrap(text.to_string(), galley_font.clone(), color)
        .size();
    let size = Vec2::new(text_size.x + 20.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(13),
        color.gamma_multiply(0.14),
        Stroke::new(1.0, color.gamma_multiply(0.7)),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        galley_font,
        color,
    );
    response
}

pub fn grade_badge(ui: &mut Ui, grade: Option<u8>, label: &str) -> egui::Response {
    let color = grade.map_or(theme::NO_DATA, theme::grade_color);
    let text = match grade {
        Some(g) => format!("{label}  {g}/5"),
        None => format!("{label}  -"),
    };
    chip(
        ui,
        &text,
        if grade.is_some() { color } else { theme::FAINT },
    )
}

pub fn stars(ui: &mut Ui, count: u8, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(size * 3.3, size), Sense::hover());
    let painter = ui.painter();
    for i in 0..3u8 {
        let color = if i < count {
            Color32::from_rgb(0xFF, 0xD5, 0x4F)
        } else {
            theme::NO_DATA
        };
        let center = Pos2::new(
            rect.left() + size * (0.5 + 1.1 * f32::from(i)),
            rect.center().y,
        );
        paint_star(painter, center, size * 0.5, color);
    }
}

/// Axes and reference line for [`line_chart`].
pub struct Chart<'a> {
    pub size: Vec2,
    pub x_range: (f64, f64),
    pub y_range: (f64, f64),
    pub y_ticks: &'a [f64],
    pub y_unit: &'a str,
    pub threshold: Option<(f64, Color32, &'a str)>,
}

/// Mini line chart. `series`: (points (x, y), colour).
pub fn line_chart(ui: &mut Ui, chart: &Chart<'_>, series: &[(Vec<(f64, f64)>, Color32)]) {
    let Chart {
        size,
        x_range,
        y_range,
        y_ticks,
        y_unit,
        threshold,
    } = *chart;
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(10), theme::DEEP);
    let plot = Rect::from_min_max(
        Pos2::new(rect.left() + 46.0, rect.top() + 12.0),
        Pos2::new(rect.right() - 12.0, rect.bottom() - 18.0),
    );
    let span_x = (x_range.1 - x_range.0).max(1e-9);
    let span_y = (y_range.1 - y_range.0).max(1e-9);
    let to_screen = |x: f64, y: f64| {
        Pos2::new(
            plot.left() + ((x - x_range.0) / span_x) as f32 * plot.width(),
            plot.bottom() - ((y - y_range.0) / span_y) as f32 * plot.height(),
        )
    };
    for &tick in y_ticks {
        let p = to_screen(x_range.0, tick);
        painter.hline(plot.x_range(), p.y, Stroke::new(1.0, theme::SURFACE_HI));
        painter.text(
            Pos2::new(plot.left() - 8.0, p.y),
            Align2::RIGHT_CENTER,
            format!("{tick:.0}{y_unit}"),
            FontId::proportional(12.0),
            theme::FAINT,
        );
    }
    if let Some((value, color, label)) = threshold {
        let p = to_screen(x_range.0, value);
        painter.add(egui::Shape::dashed_line(
            &[Pos2::new(plot.left(), p.y), Pos2::new(plot.right(), p.y)],
            Stroke::new(1.5, color),
            6.0,
            4.0,
        ));
        painter.text(
            Pos2::new(plot.right() - 4.0, p.y - 3.0),
            Align2::RIGHT_BOTTOM,
            label,
            FontId::proportional(12.0),
            color,
        );
    }
    for (points, color) in series {
        let pts: Vec<Pos2> = points.iter().map(|(x, y)| to_screen(*x, *y)).collect();
        if pts.len() >= 2 {
            painter.line(pts.clone(), Stroke::new(2.5, *color));
        }
        for p in &pts {
            painter.circle_filled(*p, 3.5, *color);
        }
    }
}
