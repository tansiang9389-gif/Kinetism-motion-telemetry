//! Kinetism colours (shared with the phone app) and egui styling.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Visuals,
};

pub const BG: Color32 = Color32::from_rgb(0x10, 0x1B, 0x22);
pub const SURFACE: Color32 = Color32::from_rgb(0x19, 0x2A, 0x33);
pub const SURFACE_HI: Color32 = Color32::from_rgb(0x22, 0x3A, 0x46);
pub const DEEP: Color32 = Color32::from_rgb(0x0B, 0x14, 0x1A);
pub const ACCENT: Color32 = Color32::from_rgb(0x57, 0xDB, 0xC4);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(0x0E, 0x2E, 0x2A);
pub const TEXT: Color32 = Color32::from_rgb(0xF0, 0xF6, 0xF8);
pub const MUTED: Color32 = Color32::from_rgb(0xB7, 0xC9, 0xD1);
pub const FAINT: Color32 = Color32::from_rgb(0x6F, 0x84, 0x8F);
pub const WARN: Color32 = Color32::from_rgb(0xFF, 0xC1, 0x5E);
pub const ERROR: Color32 = Color32::from_rgb(0xFF, 0xB4, 0xAB);
pub const GOOD: Color32 = Color32::from_rgb(0x8B, 0xE0, 0x8F);
pub const NO_DATA: Color32 = Color32::from_rgb(0x3A, 0x4B, 0x55);

/// MMT grade colours 0 (red) .. 5 (green).
pub fn grade_color(grade: u8) -> Color32 {
    match grade {
        0 => Color32::from_rgb(0xE5, 0x39, 0x35),
        1 => Color32::from_rgb(0xF4, 0x6A, 0x1E),
        2 => Color32::from_rgb(0xFB, 0xA0, 0x2C),
        3 => Color32::from_rgb(0xF2, 0xD6, 0x3A),
        4 => Color32::from_rgb(0x9C, 0xD5, 0x65),
        _ => Color32::from_rgb(0x43, 0xB0, 0x5C),
    }
}

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        let mut v = Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = SURFACE;
        v.extreme_bg_color = DEEP;
        v.faint_bg_color = SURFACE_HI;
        v.override_text_color = Some(TEXT);
        v.hyperlink_color = ACCENT;
        v.window_stroke = Stroke::new(1.0, SURFACE_HI);
        v.window_corner_radius = CornerRadius::same(12);
        v.selection.bg_fill = ACCENT.gamma_multiply(0.45);
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        let r = CornerRadius::same(8);
        v.widgets.noninteractive.bg_fill = SURFACE;
        v.widgets.noninteractive.weak_bg_fill = SURFACE;
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, SURFACE_HI);
        v.widgets.inactive.bg_fill = SURFACE_HI;
        v.widgets.inactive.weak_bg_fill = SURFACE_HI;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.inactive.corner_radius = r;
        v.widgets.hovered.bg_fill = Color32::from_rgb(0x2C, 0x4A, 0x58);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x2C, 0x4A, 0x58);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
        v.widgets.hovered.corner_radius = r;
        v.widgets.active.bg_fill = ACCENT_DARK;
        v.widgets.active.weak_bg_fill = ACCENT_DARK;
        v.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT);
        v.widgets.active.corner_radius = r;
        v.widgets.open.corner_radius = r;
        style.visuals = v;
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(14.0, 8.0);
        style.spacing.interact_size.y = 34.0;
        style.spacing.window_margin = Margin::same(16);
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(26.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(16.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(16.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(13.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(14.0, FontFamily::Monospace),
            ),
        ]
        .into();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hue_degrees(c: Color32) -> f64 {
        let (r, g, b) = (f64::from(c.r()), f64::from(c.g()), f64::from(c.b()));
        (3f64.sqrt() * (g - b)).atan2(2.0 * r - g - b).to_degrees()
    }

    #[test]
    fn grade_colours_run_from_red_to_green_and_clamp_above_five() {
        let hues: Vec<f64> = (0..=5).map(|g| hue_degrees(grade_color(g))).collect();
        assert!(
            hues.windows(2).all(|w| w[0] < w[1]),
            "each grade is a step from red towards green: {hues:?}"
        );
        assert!(hues[0].abs() < 10.0, "grade 0 is red: {hues:?}");
        assert!(
            (100.0..=140.0).contains(&hues[5]),
            "grade 5 is green: {hues:?}"
        );
        for grade in [6, 9, u8::MAX] {
            assert_eq!(grade_color(grade), grade_color(5), "grade {grade}");
        }
    }
}
