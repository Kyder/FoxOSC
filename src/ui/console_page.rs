use eframe::egui::{self, Color32, RichText, Ui};

use super::{theme, widgets, Action};
use crate::console::{LogEntry, TimedEntry};
use crate::AppState;

#[derive(Default, PartialEq, Clone, Copy)]
enum Tab {
    #[default]
    Log,
    Active,
}

#[derive(Default)]
pub(crate) struct ConsoleUiState {
    tab: Tab,
    split: bool,
    filter: String,
}

pub(crate) fn show(ui: &mut Ui, state: &AppState, ui_state: &mut ConsoleUiState, actions: &mut Vec<Action>) {
    widgets::page_header(ui, "Console", Some("OSC traffic and plugin messages"));

    // ── Toolbar ───────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        segmented(ui, &mut ui_state.tab, &[(Tab::Log, "Log"), (Tab::Active, "Active addresses")]);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut enabled = state.console.read().is_enabled();
            if ui.add(widgets::toggle(&mut enabled)).changed() {
                actions.push(Action::SetConsoleEnabled(enabled));
            }
            ui.label(RichText::new("Logging").color(theme::TEXT_WEAK));
            ui.add_space(8.0);

            match ui_state.tab {
                Tab::Log => {
                    if ui.button("Clear").clicked() {
                        actions.push(Action::ClearLog);
                    }
                    ui.toggle_value(&mut ui_state.split, "Split sent / received");
                }
                Tab::Active => {
                    if ui.button("Clear").clicked() {
                        actions.push(Action::ClearActiveAddresses);
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut ui_state.filter)
                            .hint_text("Filter addresses…")
                            .desired_width(220.0)
                            .margin(egui::vec2(10.0, 6.0)),
                    );
                }
            }
        });
    });
    ui.add_space(4.0);

    match ui_state.tab {
        Tab::Log => log_view(ui, state, ui_state.split),
        Tab::Active => active_view(ui, state, &ui_state.filter),
    }
}

fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) {
    widgets::card()
        .inner_margin(egui::Margin::same(3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (v, label) in options {
                    let selected = *value == *v;
                    let text = RichText::new(*label).color(if selected { theme::TEXT } else { theme::TEXT_WEAK });
                    let btn = egui::Button::new(text)
                        .fill(if selected { theme::ACCENT_DIM } else { Color32::TRANSPARENT })
                        .stroke(egui::Stroke::NONE);
                    if ui.add(btn).clicked() {
                        *value = *v;
                    }
                }
            });
        });
}

fn log_view(ui: &mut Ui, state: &AppState, split: bool) {
    let entries: Vec<TimedEntry> = state.console.read().entries().iter().cloned().collect();

    if !split {
        log_list(ui, "log_all", &entries.iter().collect::<Vec<_>>());
        return;
    }

    // Split: OSC sent + info/errors on the left, received on the right.
    let (received, sent): (Vec<&TimedEntry>, Vec<&TimedEntry>) = entries
        .iter()
        .partition(|e| matches!(e.entry, LogEntry::OscReceived { .. }));

    ui.columns(2, |cols| {
        widgets::section_label(&mut cols[0], "Sent & messages");
        log_list(&mut cols[0], "log_sent", &sent);
        widgets::section_label(&mut cols[1], "Received");
        log_list(&mut cols[1], "log_received", &received);
    });
}

fn log_list(ui: &mut Ui, id: &str, entries: &[&TimedEntry]) {
    widgets::card()
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(ui.available_height());

            if entries.is_empty() {
                ui.label(RichText::new("Nothing logged yet").color(theme::TEXT_FAINT));
                return;
            }

            let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
            egui::ScrollArea::both()
                .id_salt(id)
                .auto_shrink(false)
                .stick_to_bottom(true)
                .show_rows(ui, row_h, entries.len(), |ui, range| {
                    // Rows must be exactly `row_h` tall for show_rows to scroll correctly.
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.spacing_mut().interact_size.y = 0.0;
                    for e in &entries[range] {
                        log_row(ui, e);
                    }
                });
        });
}

fn log_row(ui: &mut Ui, e: &TimedEntry) {
    let (icon, color, text) = match &e.entry {
        LogEntry::Info(m) => ("ℹ", theme::TEXT_WEAK, m.clone()),
        LogEntry::Error(m) => ("✗", theme::ERROR, m.clone()),
        LogEntry::OscSent { address, value } => ("→", theme::SENT, format!("{address} = {value}")),
        LogEntry::OscReceived { address, value } => ("←", theme::RECEIVED, format!("{address} = {value}")),
    };
    let text_color = if matches!(e.entry, LogEntry::Error(_)) { theme::ERROR } else { theme::TEXT };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(RichText::new(&e.time).font(theme::mono_font()).color(theme::TEXT_FAINT));
        ui.label(RichText::new(icon).font(theme::mono_font()).color(color));
        ui.add(egui::Label::new(RichText::new(text).font(theme::mono_font()).color(text_color)).extend());
    });
}

fn active_view(ui: &mut Ui, state: &AppState, filter: &str) {
    let filter = filter.to_lowercase();
    let mut rows: Vec<(String, String)> = state
        .console
        .read()
        .get_active_addresses()
        .iter()
        .filter(|(a, _)| filter.is_empty() || a.to_lowercase().contains(&filter))
        .map(|(a, v)| (a.clone(), v.clone()))
        .collect();
    rows.sort();

    widgets::card()
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(ui.available_height());

            if rows.is_empty() {
                ui.label(RichText::new("No OSC addresses received yet").color(theme::TEXT_FAINT));
                return;
            }

            egui::ScrollArea::both().id_salt("active").auto_shrink(false).show(ui, |ui| {
                egui::Grid::new("active_grid")
                    .striped(true)
                    .num_columns(2)
                    .spacing([24.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Address").font(theme::small_font()).color(theme::TEXT_FAINT));
                        ui.label(RichText::new("Value").font(theme::small_font()).color(theme::TEXT_FAINT));
                        ui.end_row();
                        for (addr, value) in &rows {
                            ui.label(RichText::new(addr).font(theme::mono_font()).color(theme::TEXT));
                            ui.label(RichText::new(value).font(theme::mono_font()).color(theme::RECEIVED));
                            ui.end_row();
                        }
                    });
            });
        });
}
