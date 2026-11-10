//! Small reusable building blocks, all styled from `theme`.

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, Frame, Margin, Response, RichText, Sense,
    Stroke, Ui, Widget,
};

use super::theme;

/// Rounded card container.
pub fn card() -> Frame {
    Frame::new()
        .fill(theme::CARD)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(Margin::same(theme::CARD_PADDING))
}

/// Page title with an optional subtitle underneath.
pub fn page_header(ui: &mut Ui, title: &str, subtitle: Option<&str>) {
    ui.label(RichText::new(title).font(theme::heading_font()).color(theme::TEXT).strong());
    if let Some(sub) = subtitle {
        ui.add_space(-6.0);
        ui.label(RichText::new(sub).color(theme::TEXT_WEAK));
    }
    ui.add_space(8.0);
}

/// Small uppercase label used above groups of content.
pub fn section_label(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .font(theme::small_font())
            .color(theme::TEXT_FAINT)
            .strong(),
    );
}

/// Filled accent button for the main action on a page.
pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(theme::ON_ACCENT).strong())
            .fill(theme::ACCENT)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(theme::RADIUS_SMALL)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Pill-shaped tag, e.g. a version number.
pub fn chip(ui: &mut Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.15))
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).font(theme::small_font()).color(color));
        });
}

/// Big number with a caption, for live stats.
pub fn stat_tile(ui: &mut Ui, label: &str, value: &str) {
    card().show(ui, |ui| {
        ui.set_min_width(180.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(label).color(theme::TEXT_WEAK));
            ui.label(
                RichText::new(value)
                    .font(theme::stat_font())
                    .color(theme::ACCENT)
                    .strong(),
            );
        });
    });
}

/// iOS-style on/off switch.
pub fn toggle(on: &mut bool) -> impl Widget + '_ {
    move |ui: &mut Ui| {
        let size = vec2(40.0, 22.0);
        let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);

        if ui.is_rect_visible(rect) {
            let t = ui.ctx().animate_bool(response.id, *on);
            let radius = rect.height() / 2.0;
            let track = lerp_color(theme::BORDER, theme::ACCENT, t);
            let knob = lerp_color(theme::TEXT_WEAK, theme::ON_ACCENT, t);
            ui.painter().rect_filled(rect, radius, track);
            let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
            ui.painter()
                .circle_filled(pos2(x, rect.center().y), radius - 4.0, knob);
        }
        response
    }
}

/// App mark: an accent tile with an "F". Swap for an image later if you like.
pub fn logo(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(7), theme::ACCENT);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        "F",
        egui::FontId::proportional(17.0),
        theme::ON_ACCENT,
    );
}

/// One row in the sidebar navigation.
pub fn nav_item(ui: &mut Ui, icon: &str, label: &str, selected: bool, dot: Option<Color32>) -> Response {
    let size = vec2(ui.available_width(), 36.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let bg = if selected {
            theme::ACCENT_DIM
        } else if response.hovered() {
            theme::CARD
        } else {
            Color32::TRANSPARENT
        };
        painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_SMALL), bg);

        if selected {
            let bar = egui::Rect::from_min_size(rect.left_top() + vec2(0.0, 8.0), vec2(3.0, rect.height() - 16.0));
            painter.rect_filled(bar, 2.0, theme::ACCENT);
        }

        let fg = if selected { theme::TEXT } else { theme::TEXT_WEAK };
        painter.text(
            rect.left_center() + vec2(16.0, 0.0),
            Align2::LEFT_CENTER,
            icon,
            theme::title_font(),
            if selected { theme::ACCENT } else { theme::TEXT_FAINT },
        );
        let label_galley = painter.layout(
            label.to_owned(),
            egui::FontId::proportional(14.5),
            fg,
            rect.width() - 64.0,
        );
        painter.galley(
            pos2(rect.left() + 42.0, rect.center().y - label_galley.size().y / 2.0),
            label_galley,
            fg,
        );

        if let Some(color) = dot {
            painter.circle_filled(rect.right_center() - vec2(14.0, 0.0), 3.5, color);
        }
    }
    response
}

/// Table with equal-width, centred columns, each row in its own rounded band,
/// and an optional button column on the right. Returns the id of a clicked
/// row button.
pub fn table(
    ui: &mut Ui,
    columns: &[String],
    rows: &[crate::plugin_api::TableRow],
    empty_text: Option<&str>,
) -> Option<String> {
    let mut clicked = None;
    let width = ui.available_width();
    let has_buttons = rows.iter().any(|r| r.button.is_some());
    let action_w = if has_buttons { 104.0 } else { 0.0 };
    let col_w = ((width - action_w) / columns.len().max(1) as f32).max(40.0);
    let row_h = 38.0;
    let cell_center = |rect: egui::Rect, i: usize| pos2(rect.left() + col_w * (i as f32 + 0.5), rect.center().y);

    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;

        // Header
        let (head, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
        for (i, title) in columns.iter().enumerate() {
            ui.painter().text(
                cell_center(head, i),
                Align2::CENTER_CENTER,
                title.to_uppercase(),
                theme::small_font(),
                theme::TEXT_FAINT,
            );
        }

        if rows.is_empty() {
            let (r, _) = ui.allocate_exact_size(vec2(width, row_h), Sense::hover());
            ui.painter().rect_stroke(
                r,
                CornerRadius::same(theme::RADIUS_SMALL),
                Stroke::new(1.0, theme::BORDER),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                empty_text.unwrap_or("Nothing here yet"),
                egui::FontId::proportional(14.0),
                theme::TEXT_FAINT,
            );
            return;
        }

        for (n, row) in rows.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(vec2(width, row_h), Sense::hover());
            let hovered = ui.rect_contains_pointer(rect);
            let fill = if hovered {
                Color32::from_rgb(0x2c, 0x27, 0x33)
            } else if n % 2 == 0 {
                theme::CARD_HOVER
            } else {
                theme::INPUT
            };
            ui.painter().rect(
                rect,
                CornerRadius::same(theme::RADIUS_SMALL),
                fill,
                Stroke::new(1.0, if hovered { theme::BORDER } else { Color32::TRANSPARENT }),
                egui::StrokeKind::Inside,
            );

            for (i, cell) in row.cells.iter().take(columns.len()).enumerate() {
                let c = cell_center(rect, i);
                let clip = egui::Rect::from_center_size(c, vec2(col_w - 12.0, row_h));
                ui.painter().with_clip_rect(clip).text(
                    c,
                    Align2::CENTER_CENTER,
                    cell,
                    egui::FontId::proportional(14.5),
                    theme::TEXT,
                );
            }

            if let Some(btn) = &row.button {
                let b_rect = egui::Rect::from_center_size(
                    pos2(rect.right() - action_w / 2.0, rect.center().y),
                    vec2(action_w - 24.0, 26.0),
                );
                let button = egui::Button::new(RichText::new(&btn.label).font(theme::small_font()).color(theme::ERROR))
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, theme::ERROR.gamma_multiply(0.45)))
                    .corner_radius(CornerRadius::same(theme::RADIUS_SMALL));
                if ui.put(b_rect, button).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    clicked = Some(btn.id.clone());
                }
            }
        }
    });

    clicked
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// 1234567 -> "1 234 567"
pub fn format_with_spaces(num: u64) -> String {
    let s = num.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::format_with_spaces;

    #[test]
    fn spaces() {
        assert_eq!(format_with_spaces(0), "0");
        assert_eq!(format_with_spaces(999), "999");
        assert_eq!(format_with_spaces(1000), "1 000");
        assert_eq!(format_with_spaces(1234567), "1 234 567");
    }
}
