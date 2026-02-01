use gtk4::prelude::*;
use gtk4::Application;
use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;

use osc_app_core::{AppState, osc_manager::OscManager, ui::MainWindow};

fn main() -> Result<()> {
    env_logger::init();
    
    // Initialize GTK
    let app = Application::builder()
        .application_id("com.example.fox-osc")
        .build();
    
    app.connect_activate(|app| {
        if let Err(e) = setup_app(app) {
            eprintln!("Failed to setup application: {}", e);
            std::process::exit(1);
        }
    });
    
    app.run();
    
    Ok(())
}

fn setup_app(app: &Application) -> Result<()> {
    // Create application state
    let app_state = Arc::new(AppState::new()?);
    
    // Set console enabled from config
    {
        let config = app_state.config.read();
        app_state.console.write().set_enabled(config.ui.console_enabled);
    }
    
    // Initialize OSC manager
    let config = app_state.config.read();
    let osc_manager = Arc::new(OscManager::new(
        &config.osc.bind_address,
        &config.osc.target_address,
        app_state.console.clone(),
    )?);
    drop(config);
    
    // Load WASM plugins
    app_state.plugin_loader.write().load_all(
        osc_manager.clone(),
        app_state.console.clone(),
        app_state.config.clone(),
    )?;
    
    // Start plugins based on their saved enabled state (default: on)
    let mut loader = app_state.plugin_loader.write();
    for plugin in loader.plugins_mut() {
        let enabled = app_state.config.read()
            .get_plugin_setting(plugin.info().name.as_str(), "enabled")
            .map(|v| v != "false")
            .unwrap_or(true);
        
        if enabled {
            if let Err(e) = plugin.start() {
                app_state.console.write().log_error(&format!("Failed to start plugin: {}", e));
            }
        } else {
            app_state.console.write().log_info(&format!("Plugin '{}' is disabled, skipping", plugin.info().name));
        }
    }
    drop(loader);
    
    // Create main window
    let _main_window = MainWindow::new(app, app_state.clone());
    
    // Setup plugin update loop (100ms tick)
    let app_state_clone = app_state.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        let mut loader = app_state_clone.plugin_loader.write();
        for plugin in loader.plugins_mut() {
            if let Err(e) = plugin.update() {
                app_state_clone.console.write().log_error(&format!("Plugin update error: {}", e));
            }
        }
        glib::ControlFlow::Continue
    });
    
    Ok(())
}