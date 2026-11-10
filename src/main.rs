use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use osc_app_core::{osc_manager::OscManager, ui::FoxApp, AppState};

fn main() -> Result<()> {
    env_logger::init();

    let app_state = Arc::new(AppState::new()?);

    // Set console enabled from config
    {
        let config = app_state.config.read();
        app_state.console.write().set_enabled(config.ui.console_enabled);
    }

    // Initialize OSC manager
    let osc_manager = {
        let config = app_state.config.read();
        Arc::new(OscManager::new(
            &config.osc.bind_address,
            &config.osc.target_address,
            app_state.console.clone(),
        )?)
    };

    // Load WASM plugins
    app_state.plugin_loader.write().load_all(
        osc_manager.clone(),
        app_state.console.clone(),
        app_state.config.clone(),
    )?;

    // Start plugins based on their saved enabled state (default: on)
    {
        let mut loader = app_state.plugin_loader.write();
        for plugin in loader.plugins_mut() {
            let enabled = app_state
                .config
                .read()
                .get_plugin_setting(plugin.info().name.as_str(), "enabled")
                .map(|v| v != "false")
                .unwrap_or(true);

            if enabled {
                if let Err(e) = plugin.start() {
                    app_state.console.write().log_error(&format!("Failed to start plugin: {}", e));
                }
            } else {
                app_state
                    .console
                    .write()
                    .log_info(&format!("Plugin '{}' is disabled, skipping", plugin.info().name));
            }
        }
    }

    // Plugin update loop (100 ms tick) on its own thread. It used to run on the
    // GTK main loop; now the UI only draws and plugins keep ticking even while
    // the window is minimised and not repainting.
    let shutdown = Arc::new(AtomicBool::new(false));
    let tick_thread = {
        let app_state = app_state.clone();
        let shutdown = shutdown.clone();
        std::thread::Builder::new()
            .name("plugin-tick".into())
            .spawn(move || {
                while !shutdown.load(Ordering::Relaxed) {
                    {
                        let mut loader = app_state.plugin_loader.write();
                        for plugin in loader.plugins_mut() {
                            if let Err(e) = plugin.update() {
                                app_state
                                    .console
                                    .write()
                                    .log_error(&format!("Plugin update error: {}", e));
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            })?
    };

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Fox OSC")
            .with_app_id("fox-osc")
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([640.0, 420.0]),
        ..Default::default()
    };

    let ui_state = app_state.clone();
    let result = eframe::run_native(
        "Fox OSC",
        options,
        Box::new(move |cc| Ok(Box::new(FoxApp::new(cc, ui_state)))),
    );

    // Window closed: stop ticking, then let plugins clean up.
    shutdown.store(true, Ordering::Relaxed);
    let _ = tick_thread.join();
    for plugin in app_state.plugin_loader.write().plugins_mut() {
        let _ = plugin.stop();
    }

    result.map_err(|e| anyhow::anyhow!("UI error: {e}"))
}
