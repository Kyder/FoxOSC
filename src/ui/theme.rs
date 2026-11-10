//! All visual decisions live here. Change a colour or a radius in this file
//! and the whole app follows — no widget hard-codes its own look.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle, Theme,
    Visuals,
};

// ── Palette ────────────────────────────────────────────────────────────────
// Warm "fox" accent on a deep, slightly purple dark base.

pub const BG: Color32 = Color32::from_rgb(0x13, 0x11, 0x16); // window / central area
pub const SIDEBAR: Color32 = Color32::from_rgb(0x19, 0x16, 0x1d);
pub const CARD: Color32 = Color32::from_rgb(0x20, 0x1c, 0x25);
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x28, 0x23, 0x2e);
pub const INPUT: Color32 = Color32::from_rgb(0x15, 0x13, 0x19);
pub const BORDER: Color32 = Color32::from_rgb(0x2f, 0x2a, 0x36);

pub const TEXT: Color32 = Color32::from_rgb(0xec, 0xe6, 0xf0);
pub const TEXT_WEAK: Color32 = Color32::from_rgb(0x9a, 0x91, 0xa5);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x6b, 0x63, 0x75);

pub const ACCENT: Color32 = Color32::from_rgb(0xff, 0x8a, 0x3d); // fox orange
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0xff, 0xa2, 0x63);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x4a, 0x2d, 0x1c); // accent on dark, for fills
pub const ON_ACCENT: Color32 = Color32::from_rgb(0x1a, 0x10, 0x08);

pub const OK: Color32 = Color32::from_rgb(0x5f, 0xd3, 0x8d);
pub const ERROR: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);
pub const SENT: Color32 = Color32::from_rgb(0xff, 0xb3, 0x7a);
pub const RECEIVED: Color32 = Color32::from_rgb(0x6c, 0xb6, 0xff);

// ── Shape ──────────────────────────────────────────────────────────────────

pub const RADIUS: u8 = 10;
pub const RADIUS_SMALL: u8 = 6;
pub const PAGE_MARGIN: i8 = 24;
pub const CARD_PADDING: i8 = 16;
pub const SIDEBAR_WIDTH: f32 = 210.0;

// ── Type scale ─────────────────────────────────────────────────────────────

pub fn heading_font() -> FontId {
    FontId::new(24.0, FontFamily::Proportional)
}
pub fn title_font() -> FontId {
    FontId::new(16.0, FontFamily::Proportional)
}
pub fn stat_font() -> FontId {
    FontId::new(30.0, FontFamily::Proportional)
}
pub fn small_font() -> FontId {
    FontId::new(12.0, FontFamily::Proportional)
}
pub fn mono_font() -> FontId {
    FontId::new(13.0, FontFamily::Monospace)
}

/// Install the theme. Called once at startup.
pub fn apply(ctx: &egui::Context) {
    // The app is dark-only by design; ignore the OS light/dark preference.
    ctx.set_theme(egui::ThemePreference::Dark);

    ctx.style_mut_of(Theme::Dark, |style| {
        style.text_styles = [
            (TextStyle::Heading, heading_font()),
            (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.5, FontFamily::Proportional)),
            (TextStyle::Small, small_font()),
            (TextStyle::Monospace, mono_font()),
        ]
        .into();

        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(10.0, 10.0);
        s.button_padding = egui::vec2(14.0, 7.0);
        s.interact_size.y = 30.0;
        s.window_margin = Margin::same(CARD_PADDING);

        style.visuals = visuals();
    });
}

fn visuals() -> Visuals {
    let mut v = Visuals::dark();
    let r = CornerRadius::same(RADIUS_SMALL);

    v.override_text_color = None;
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(RADIUS);
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.menu_corner_radius = r;
    v.faint_bg_color = CARD_HOVER; // striped grid rows
    v.extreme_bg_color = INPUT; // text edits, scroll backgrounds
    v.text_edit_bg_color = Some(INPUT);
    v.code_bg_color = INPUT;
    v.hyperlink_color = ACCENT;
    v.error_fg_color = ERROR;
    v.warn_fg_color = ACCENT;
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0, ACCENT);

    let w = &mut v.widgets;

    w.noninteractive.bg_fill = CARD;
    w.noninteractive.weak_bg_fill = CARD;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER); // separators
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = CARD_HOVER;
    w.inactive.weak_bg_fill = CARD_HOVER;
    w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = Color32::from_rgb(0x33, 0x2d, 0x3a);
    w.hovered.weak_bg_fill = Color32::from_rgb(0x33, 0x2d, 0x3a);
    w.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    w.hovered.fg_stroke = Stroke::new(1.5, TEXT);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = ACCENT_DIM;
    w.active.weak_bg_fill = ACCENT_DIM;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.5, TEXT);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open = w.active;

    v
}
