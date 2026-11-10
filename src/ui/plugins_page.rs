use eframe::egui::{self, RichText, Ui};

use super::{theme, widgets, Action, Page, PluginView};
use crate::AppState;

pub(crate) fn show(ui: &mut Ui, state: &AppState, plugins: &[PluginView], actions: &mut Vec<Action>) {
    let mut dir = state.plugin_loader.read().plugins_dir().display().to_string();
    if let Some(home) = dirs::home_dir().map(|h| h.display().to_string()) {
        if let Some(rest) = dir.strip_prefix(&home) {
            dir = format!("~{rest}");
        }
    }
    widgets::page_header(ui, "Plugins", Some(&format!("WebAssembly plugins from {dir}")));

    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if plugins.is_empty() {
            widgets::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("No plugins loaded").font(theme::title_font()).color(theme::TEXT));
                ui.label(RichText::new("Drop .wasm files into the folder above and restart Fox OSC.").color(theme::TEXT_WEAK));
            });
            return;
        }

        for p in plugins {
            plugin_card(ui, p, actions);
            ui.add_space(2.0);
        }

        ui.add_space(12.0);
        widgets::section_label(ui, "Adding plugins");
        ui.label(
            RichText::new("Place .wasm files in the plugins folder and restart the app. They load automatically.")
                .color(theme::TEXT_WEAK),
        );
    });
}

fn plugin_card(ui: &mut Ui, p: &PluginView, actions: &mut Vec<Action>) {
    widgets::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            // Right side first so the text can wrap in whatever is left.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut running = p.running;
                if ui.add(widgets::toggle(&mut running)).changed() {
                    actions.push(Action::SetPluginRunning { idx: p.idx, running });
                }
                let (txt, col) = if p.running { ("Running", theme::OK) } else { ("Stopped", theme::TEXT_FAINT) };
                ui.label(RichText::new(txt).font(theme::small_font()).color(col));

                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&p.info.name).font(theme::title_font()).color(theme::TEXT).strong());
                        widgets::chip(ui, &format!("v{}", p.info.version), theme::TEXT_WEAK);
                    });
                    ui.add(egui::Label::new(RichText::new(&p.info.description).color(theme::TEXT_WEAK)).wrap());
                    if p.ui_config.is_some() {
                        let link = ui.link(RichText::new("⚙ Open settings").font(theme::small_font()));
                        if link.clicked() {
                            actions.push(Action::Navigate(Page::Plugin(p.idx)));
                        }
                    }
                });
            });
        });
    });
}
