//! egui front end.
//!
//! The UI never owns application state. Each frame it takes a cheap snapshot
//! of the plugins, draws, and collects what the user did as [`Action`]s, which
//! are applied after drawing. Because the whole screen is rebuilt every frame
//! from that snapshot, a plugin changing its `UiConfig` (or a config value
//! changing behind our back) shows up on the next frame with no manual
//! widget rebuilding.

mod console_page;
mod plugin_page;
mod plugins_page;
pub mod theme;
pub mod widgets;

use eframe::egui::{self, Margin, RichText};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::plugin_api::{PluginInfo, UiConfig, UiEvent};
use crate::AppState;

/// How often to redraw when nothing is happening, so live values and the
/// console stay fresh. Nothing is drawn while the window is minimised.
const IDLE_REPAINT: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    Plugins,
    Console,
    Plugin(usize),
}

/// Read-only copy of what the UI needs to know about a plugin this frame.
pub(crate) struct PluginView {
    pub idx: usize,
    pub info: PluginInfo,
    pub ui_config: Option<UiConfig>,
    pub running: bool,
}

/// Something the user did, applied after the frame is drawn.
pub(crate) enum Action {
    Navigate(Page),
    SetPluginRunning { idx: usize, running: bool },
    PluginButton { idx: usize, id: String },
    PluginToggle { idx: usize, id: String, value: bool },
    ApplySettings { idx: usize, values: Vec<(String, String)> },
    SetConsoleEnabled(bool),
    ClearLog,
    ClearActiveAddresses,
}

pub struct FoxApp {
    state: Arc<AppState>,
    page: Page,
    console: console_page::ConsoleUiState,
    /// Text-input buffers, keyed by (plugin index, element id).
    inputs: HashMap<(usize, String), String>,
}

impl FoxApp {
    pub fn new(cc: &eframe::CreationContext<'_>, state: Arc<AppState>) -> Self {
        theme::apply(&cc.egui_ctx);
        Self {
            state,
            page: Page::Plugins,
            console: Default::default(),
            inputs: HashMap::new(),
        }
    }

    fn snapshot_plugins(&self) -> Vec<PluginView> {
        let loader = self.state.plugin_loader.read();
        loader
            .plugins()
            .iter()
            .enumerate()
            .map(|(idx, p)| PluginView {
                idx,
                info: p.info().clone(),
                ui_config: p.ui_config().cloned(),
                running: p.is_running(),
            })
            .collect()
    }

    fn sidebar(&self, ui: &mut egui::Ui, plugins: &[PluginView], actions: &mut Vec<Action>) {
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            widgets::logo(ui);
            ui.label(
                RichText::new("Fox OSC")
                    .font(theme::title_font())
                    .color(theme::TEXT)
                    .strong(),
            );
        });
        ui.add_space(18.0);

        let mut nav = |ui: &mut egui::Ui, icon: &str, label: &str, page: Page, dot| {
            if widgets::nav_item(ui, icon, label, self.page == page, dot).clicked() {
                actions.push(Action::Navigate(page));
            }
        };

        nav(ui, "⊞", "Plugins", Page::Plugins, None);
        nav(ui, "☰", "Console", Page::Console, None);

        let with_ui: Vec<_> = plugins.iter().filter(|p| p.ui_config.is_some()).collect();
        if !with_ui.is_empty() {
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                widgets::section_label(ui, "Plugin settings");
            });
            for p in with_ui {
                let title = p
                    .ui_config
                    .as_ref()
                    .map(|c| c.title.as_str())
                    .unwrap_or(&p.info.name);
                let dot = Some(if p.running { theme::OK } else { theme::TEXT_FAINT });
                nav(ui, "⚙", title, Page::Plugin(p.idx), dot);
            }
        }
    }

    /// Connection info pinned to the bottom of the sidebar.
    fn sidebar_footer(&self, ui: &mut egui::Ui, plugins: &[PluginView]) {
        let cfg = self.state.config.read();
        let running = plugins.iter().filter(|p| p.running).count();
        egui::Panel::bottom("sidebar_footer")
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().inner_margin(Margin { left: 14, right: 0, top: 8, bottom: 14 }))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for line in [
                    format!("{running}/{} plugins running", plugins.len()),
                    format!("in   {}", cfg.osc.bind_address),
                    format!("out  {}", cfg.osc.target_address),
                ] {
                    ui.label(RichText::new(line).font(theme::small_font()).color(theme::TEXT_FAINT));
                }
            });
    }

    fn apply_actions(&mut self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Navigate(page) => self.page = page,

                Action::SetPluginRunning { idx, running } => {
                    let mut loader = self.state.plugin_loader.write();
                    let Some(plugin) = loader.plugins_mut().get_mut(idx) else { continue };
                    let name = plugin.info().name.clone();
                    let result = if running { plugin.start() } else { plugin.stop() };
                    drop(loader);

                    if let Err(e) = result {
                        let verb = if running { "start" } else { "stop" };
                        self.state.console.write().log_error(&format!("Failed to {verb} plugin: {e}"));
                    } else {
                        let mut config = self.state.config.write();
                        config.set_plugin_setting(&name, "enabled", if running { "true" } else { "false" });
                        if let Err(e) = config.save() {
                            self.state.console.write().log_error(&format!("Failed to save config: {e}"));
                        }
                    }
                }

                Action::PluginToggle { idx, id, value } => {
                    self.send_event(idx, &UiEvent::ToggleChanged { id, value });
                    if let Some(p) = self.state.plugin_loader.write().plugins_mut().get_mut(idx) {
                        let _ = p.refresh_ui_config();
                    }
                }

                Action::PluginButton { idx, id } => {
                    self.send_event(idx, &UiEvent::ButtonClicked { id });
                    // A button can change the plugin's UI (e.g. Delete removes a row).
                    if let Some(p) = self.state.plugin_loader.write().plugins_mut().get_mut(idx) {
                        let _ = p.refresh_ui_config();
                    }
                }

                Action::ApplySettings { idx, values } => {
                    self.send_event(idx, &UiEvent::ApplySettings { values });
                    let name = {
                        let mut loader = self.state.plugin_loader.write();
                        loader.plugins_mut().get_mut(idx).map(|p| {
                            // Pick up any change in the plugin's UI (e.g. a new message list).
                            let _ = p.refresh_ui_config();
                            p.info().name.clone()
                        })
                    };
                    // Drop cached input text so fields reload from the saved config.
                    self.inputs.retain(|(i, _), _| *i != idx);
                    if let Some(name) = name {
                        self.state.console.write().log_info(&format!("Changes applied to {name}"));
                    }
                }

                Action::SetConsoleEnabled(enabled) => {
                    self.state.console.write().set_enabled(enabled);
                    let mut config = self.state.config.write();
                    config.ui.console_enabled = enabled;
                    if let Err(e) = config.save() {
                        self.state.console.write().log_error(&format!("Failed to save config: {e}"));
                    }
                }

                Action::ClearLog => self.state.console.write().clear(),
                Action::ClearActiveAddresses => self.state.console.write().clear_active_addresses(),
            }
        }
    }

    fn send_event(&self, idx: usize, event: &UiEvent) {
        let Ok(json) = serde_json::to_string(event) else { return };
        let mut loader = self.state.plugin_loader.write();
        if let Some(plugin) = loader.plugins_mut().get_mut(idx) {
            if let Err(e) = plugin.send_ui_event(&json) {
                drop(loader);
                self.state.console.write().log_error(&format!("Failed to send UI event: {e}"));
            }
        }
    }
}

impl eframe::App for FoxApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let plugins = self.snapshot_plugins();
        let mut actions = Vec::new();

        // A plugin page whose plugin vanished falls back to the plugin list.
        if let Page::Plugin(idx) = self.page {
            if plugins.get(idx).and_then(|p| p.ui_config.as_ref()).is_none() {
                self.page = Page::Plugins;
            }
        }

        egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(theme::SIDEBAR_WIDTH)
            .frame(
                egui::Frame::new()
                    .fill(theme::SIDEBAR)
                    .inner_margin(Margin::symmetric(10, 0)),
            )
            .show(ui, |ui| {
                self.sidebar_footer(ui, &plugins);
                self.sidebar(ui, &plugins, &mut actions);
            });

        egui::CentralPanel::default_margins()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::same(theme::PAGE_MARGIN)),
            )
            .show(ui, |ui| match self.page {
                Page::Plugins => plugins_page::show(ui, &self.state, &plugins, &mut actions),
                Page::Console => console_page::show(ui, &self.state, &mut self.console, &mut actions),
                Page::Plugin(idx) => {
                    plugin_page::show(ui, &self.state, &plugins[idx], &mut self.inputs, &mut actions)
                }
            });

        self.apply_actions(actions);
        ui.ctx().request_repaint_after(IDLE_REPAINT);
    }
}
