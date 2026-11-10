//! Generic settings page built from a plugin's `UiConfig`.

use eframe::egui::{self, RichText, Ui};
use std::collections::HashMap;

use super::{theme, widgets, Action, PluginView};
use crate::plugin_api::UiElement;
use crate::AppState;

pub(crate) fn show(
    ui: &mut Ui,
    state: &AppState,
    p: &PluginView,
    inputs: &mut HashMap<(usize, String), String>,
    actions: &mut Vec<Action>,
) {
    let Some(cfg) = &p.ui_config else { return };
    widgets::page_header(ui, &cfg.title, Some(&p.info.description));

    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        // ── Live values ───────────────────────────────────────────────────
        let live: Vec<(String, String)> = cfg
            .elements
            .iter()
            .filter_map(|e| match e {
                UiElement::LiveValue { label, config_key } => Some((label.clone(), config_key.clone())),
                _ => None,
            })
            .collect();

        if !live.is_empty() {
            let config = state.config.read();
            ui.horizontal_wrapped(|ui| {
                for (label, key) in &live {
                    let raw = config.get_plugin_setting(&p.info.name, key).unwrap_or_default();
                    let shown = raw
                        .parse::<u64>()
                        .map(widgets::format_with_spaces)
                        .unwrap_or_else(|_| if raw.is_empty() { "—".into() } else { raw.clone() });
                    widgets::stat_tile(ui, label, &shown);
                }
            });
            ui.add_space(8.0);
        }

        // ── Settings ──────────────────────────────────────────────────────
        let has_settings = cfg.elements.iter().any(|e| !matches!(e, UiElement::LiveValue { .. }));
        if !has_settings {
            return;
        }

        widgets::card().show(ui, |ui| {
            ui.set_width(ui.available_width());

            for element in &cfg.elements {
                match element {
                    UiElement::Label { text } => {
                        ui.add(egui::Label::new(RichText::new(text).color(theme::TEXT)).wrap());
                    }
                    UiElement::TextInput { id, label, default_value, placeholder } => {
                        let buf = inputs.entry((p.idx, id.clone())).or_insert_with(|| {
                            // Same key scheme the plugins already use.
                            state
                                .config
                                .read()
                                .get_plugin_setting(&p.info.name, &format!("{id}_address"))
                                .unwrap_or_else(|| default_value.clone())
                        });
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(150.0, 30.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.set_min_width(150.0);
                                    ui.label(RichText::new(label).color(theme::TEXT_WEAK));
                                },
                            );
                            ui.add(
                                egui::TextEdit::singleline(buf)
                                    .hint_text(placeholder)
                                    .desired_width(f32::INFINITY)
                                    .margin(egui::vec2(10.0, 6.0)),
                            );
                        });
                    }
                    UiElement::Button { id, label } => {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            if ui.button(label).clicked() {
                                actions.push(Action::PluginButton { idx: p.idx, id: id.clone() });
                            }
                        });
                    }
                    UiElement::Separator => {
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(4.0);
                    }
                    UiElement::Toggle { id, label, value } => {
                        ui.horizontal(|ui| {
                            let mut v = *value;
                            if ui.add(widgets::toggle(&mut v)).changed() {
                                actions.push(Action::PluginToggle { idx: p.idx, id: id.clone(), value: v });
                            }
                            ui.label(RichText::new(label).color(theme::TEXT));
                        });
                    }
                    UiElement::Table { columns, rows, empty_text } => {
                        if let Some(id) = widgets::table(ui, columns, rows, empty_text.as_deref()) {
                            actions.push(Action::PluginButton { idx: p.idx, id });
                        }
                    }
                    UiElement::LiveValue { .. } => {} // drawn above
                }
            }

            ui.add_space(6.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                if widgets::primary_button(ui, "Apply changes").clicked() {
                    let values = cfg
                        .elements
                        .iter()
                        .filter_map(|e| match e {
                            UiElement::TextInput { id, .. } => inputs
                                .get(&(p.idx, id.clone()))
                                .map(|v| (id.clone(), v.clone())),
                            _ => None,
                        })
                        .collect();
                    actions.push(Action::ApplySettings { idx: p.idx, values });
                }
            });
        });
    });
}
