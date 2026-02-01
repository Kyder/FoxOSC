use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, Entry, Label, Notebook, 
    Orientation, Switch, Widget,
};
use std::sync::Arc;
use std::collections::HashMap;
use glib;

use crate::AppState;
use crate::plugin_api::{UiElement, UiEvent};
use crate::console::create_console_ui;

#[allow(dead_code)]
pub struct MainWindow {
    window: ApplicationWindow,
    app_state: Arc<AppState>,
    console_switch: Switch,
}

impl MainWindow {
    pub fn new(app: &Application, app_state: Arc<AppState>) -> Self {
        let window = ApplicationWindow::new(app);
        window.set_title(Some("Fox OSC"));
        window.set_default_size(800, 600);
        
        let notebook = Notebook::new();
        
        // Console Log tab with new two-tab console
        let (console_view, console_switch, _console_views) = create_console_ui(app_state.console.clone());
        notebook.append_page(&console_view, Some(&Label::new(Some("Console Log"))));
        
        // Plugins tab
        let plugins_tab = Self::create_plugins_tab(app_state.clone());
        notebook.append_page(&plugins_tab, Some(&Label::new(Some("Plugins"))));
        
        // Add plugin-specific tabs from UI configs
        let plugin_loader = app_state.plugin_loader.read();
        for (idx, plugin) in plugin_loader.plugins().iter().enumerate() {
            if let Some(ui_config) = plugin.ui_config() {
                let plugin_tab = Self::create_plugin_ui_tab(ui_config, idx, plugin.info().name.clone(), app_state.clone());
                notebook.append_page(&plugin_tab, Some(&Label::new(Some(&ui_config.title))));
            }
        }
        drop(plugin_loader);
        
        window.set_child(Some(&notebook));
        
        // Connect console switch to save config
        let app_state_clone = app_state.clone();
        let console_switch_clone = console_switch.clone();
        console_switch.connect_state_set(move |_, enabled| {
            app_state_clone.console.write().set_enabled(enabled);
            
            // Save to config
            let mut config = app_state_clone.config.write();
            config.ui.console_enabled = enabled;
            if let Err(e) = config.save() {
                app_state_clone.console.write().log_error(&format!("Failed to save config: {}", e));
            }
            
            glib::Propagation::Proceed
        });
        
        window.present();
        
        Self {
            window,
            app_state,
            console_switch: console_switch_clone,
        }
    }
    
    fn create_plugin_ui_tab(ui_config: &crate::plugin_api::UiConfig, plugin_idx: usize, plugin_name: String, app_state: Arc<AppState>) -> Widget {
        let vbox = GtkBox::new(Orientation::Vertical, 10);
        vbox.set_margin_top(20);
        vbox.set_margin_bottom(20);
        vbox.set_margin_start(20);
        vbox.set_margin_end(20);
        
        // Store input widgets by ID
        let mut input_widgets: HashMap<String, Entry> = HashMap::new();
        
        // SPECIAL: For Boop Counter, add live updating counters at the top
        if plugin_name == "Boop Counter" {
            let title_label = Label::new(None);
            title_label.set_markup("<span size='x-large' weight='bold'>Boop Statistics</span>");
            title_label.set_halign(gtk4::Align::Start);
            vbox.append(&title_label);
            
            let today_label = Label::new(Some("Today: Loading..."));
            today_label.set_halign(gtk4::Align::Start);
            
            let total_label = Label::new(Some("Total: Loading..."));
            total_label.set_halign(gtk4::Align::Start);
            
            vbox.append(&today_label);
            vbox.append(&total_label);
            
            let separator = gtk4::Separator::new(Orientation::Horizontal);
            separator.set_margin_top(10);
            separator.set_margin_bottom(10);
            vbox.append(&separator);
            
            // Timer to update counts every second
            let app_state_timer = app_state.clone();
            let today_timer = today_label.clone();
            let total_timer = total_label.clone();
            
            glib::timeout_add_seconds_local(1, move || {
                let config = app_state_timer.config.read();
                
                let today = config.get_plugin_setting("Boop Counter", "today_boops")
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);
                
                let total = config.get_plugin_setting("Boop Counter", "total_boops")
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);
                
                today_timer.set_markup(&format!("<span size='large'>Today Boops: <b>{}</b></span>", today));
                total_timer.set_markup(&format!("<span size='large'>Total Boops: <b>{}</b></span>", total));
                
                glib::ControlFlow::Continue
            });
        }
        
        for element in &ui_config.elements {
            match element {
                UiElement::Label { text } => {
                    let label = Label::new(Some(text));
                    label.set_halign(gtk4::Align::Start);
                    vbox.append(&label);
                }
                UiElement::TextInput { id, label, default_value, placeholder } => {
                    let hbox = GtkBox::new(Orientation::Horizontal, 10);
                    
                    let label_widget = Label::new(Some(label));
                    label_widget.set_width_chars(15);
                    label_widget.set_halign(gtk4::Align::Start);
                    hbox.append(&label_widget);
                    
                    let entry = Entry::new();
                    
                    // Load saved value from config or use default
                    let config = app_state.config.read();
                    let config_key = format!("{}_address", id);
                    if let Some(saved_value) = config.get_plugin_setting(&plugin_name, &config_key) {
                        entry.set_text(&saved_value);
                    } else {
                        entry.set_text(default_value);
                    }
                    drop(config);
                    
                    entry.set_placeholder_text(Some(placeholder));
                    entry.set_hexpand(true);
                    hbox.append(&entry);
                    
                    input_widgets.insert(id.clone(), entry.clone());
                    vbox.append(&hbox);
                }
                UiElement::Button { id, label } => {
                    let button = Button::with_label(label);
                    button.set_halign(gtk4::Align::End);
                    
                    let app_state_clone = app_state.clone();
                    let button_id = id.clone();
                    button.connect_clicked(move |_| {
                        // Send button click event to plugin
                        let event = UiEvent::ButtonClicked { id: button_id.clone() };
                        if let Ok(event_json) = serde_json::to_string(&event) {
                            let mut loader = app_state_clone.plugin_loader.write();
                            if let Some(plugin) = loader.plugins_mut().get_mut(plugin_idx) {
                                if let Err(e) = plugin.send_ui_event(&event_json) {
                                    app_state_clone.console.write().log_error(&format!("Failed to send UI event: {}", e));
                                }
                            }
                        }
                    });
                    
                    vbox.append(&button);
                }
                UiElement::Separator => {
                    let separator = gtk4::Separator::new(Orientation::Horizontal);
                    separator.set_margin_top(10);
                    separator.set_margin_bottom(10);
                    vbox.append(&separator);
                }
            }
        }
        
        // Add an "Apply" button at the bottom to send all values
        let apply_button = Button::with_label("Apply Changes");
        apply_button.set_halign(gtk4::Align::End);
        apply_button.set_margin_top(10);
        
        let app_state_clone = app_state.clone();
        apply_button.connect_clicked(move |_| {
            // Collect all input values
            let mut values = Vec::new();
            for (id, entry) in &input_widgets {
                values.push((id.clone(), entry.text().to_string()));
            }
            
            // Send apply event to plugin
            let event = UiEvent::ApplySettings { values };
            if let Ok(event_json) = serde_json::to_string(&event) {
                let mut loader = app_state_clone.plugin_loader.write();
                if let Some(plugin) = loader.plugins_mut().get_mut(plugin_idx) {
                    if let Err(e) = plugin.send_ui_event(&event_json) {
                        app_state_clone.console.write().log_error(&format!("Failed to send UI event: {}", e));
                    }
                }
            }
        });
        
        vbox.append(&apply_button);
        
        vbox.upcast::<Widget>()
    }
    
    
    fn create_plugins_tab(app_state: Arc<AppState>) -> Widget {
        let vbox = GtkBox::new(Orientation::Vertical, 10);
        vbox.set_margin_top(20);
        vbox.set_margin_bottom(20);
        vbox.set_margin_start(20);
        vbox.set_margin_end(20);
        
        let title = Label::new(None);
        title.set_markup("<span size='x-large' weight='bold'>Active Plugins (WASM)</span>");
        title.set_halign(gtk4::Align::Start);
        vbox.append(&title);
        
        let subtitle = Label::new(Some("WebAssembly plugins loaded from ~/.config/fox-osc/plugins/"));
        subtitle.set_halign(gtk4::Align::Start);
        subtitle.set_wrap(true);
        vbox.append(&subtitle);
        
        // Separator
        let separator = gtk4::Separator::new(Orientation::Horizontal);
        separator.set_margin_top(10);
        separator.set_margin_bottom(10);
        vbox.append(&separator);
        
        let plugin_loader = app_state.plugin_loader.read();
        let plugins = plugin_loader.plugins();
        
        if plugins.is_empty() {
            let empty_label = Label::new(Some("No plugins loaded. Place .wasm files in ~/.config/fox-osc/plugins/"));
            empty_label.set_halign(gtk4::Align::Start);
            vbox.append(&empty_label);
        } else {
            for (idx, plugin) in plugins.iter().enumerate() {
                let plugin_box = GtkBox::new(Orientation::Horizontal, 10);
                plugin_box.set_margin_top(10);
                plugin_box.set_margin_bottom(10);
                
                let info = plugin.info();
                
                // Left side - Plugin info
                let info_vbox = GtkBox::new(Orientation::Vertical, 5);
                
                // Plugin name and version
                let name_label = Label::new(None);
                name_label.set_markup(&format!("<span size='large' weight='bold'>{}</span> <span size='small'>v{}</span>", 
                    info.name, info.version));
                name_label.set_halign(gtk4::Align::Start);
                info_vbox.append(&name_label);
                
                // Description
                let desc_label = Label::new(Some(&info.description));
                desc_label.set_halign(gtk4::Align::Start);
                desc_label.set_wrap(true);
                info_vbox.append(&desc_label);
                
                // UI config available?
                if plugin.ui_config().is_some() {
                    let ui_label = Label::new(Some("\u{2699} Has configuration tab"));
                    ui_label.set_halign(gtk4::Align::Start);
                    info_vbox.append(&ui_label);
                }
                
                plugin_box.append(&info_vbox);
                
                // Right side - on/off switch
                let switch = Switch::new();
                switch.set_active(plugin.is_running());
                switch.set_valign(gtk4::Align::Center);
                switch.set_margin_start(20);
                
                let app_state_clone = app_state.clone();
                switch.connect_state_set(move |_, enabled| {
                    let mut loader = app_state_clone.plugin_loader.write();
                    if let Some(plugin) = loader.plugins_mut().get_mut(idx) {
                        let plugin_name = plugin.info().name.clone();
                        
                        let result = if enabled {
                            plugin.start()
                        } else {
                            plugin.stop()
                        };
                        
                        if let Err(e) = result {
                            let action = if enabled { "start" } else { "stop" };
                            app_state_clone.console.write().log_error(
                                &format!("Failed to {} plugin: {}", action, e)
                            );
                        } else {
                            // Save enabled state to config
                            let mut config = app_state_clone.config.write();
                            config.set_plugin_setting(&plugin_name, "enabled", if enabled { "true" } else { "false" });
                            if let Err(e) = config.save() {
                                app_state_clone.console.write().log_error(&format!("Failed to save config: {}", e));
                            }
                        }
                    }
                    glib::Propagation::Proceed
                });
                
                plugin_box.append(&switch);
                
                vbox.append(&plugin_box);
                
                // Separator
                let separator = gtk4::Separator::new(Orientation::Horizontal);
                separator.set_margin_top(5);
                vbox.append(&separator);
            }
        }
        drop(plugin_loader);
        
        // Info about adding plugins
        let info_box = GtkBox::new(Orientation::Vertical, 5);
        info_box.set_margin_top(20);
        
        let info_title = Label::new(None);
        info_title.set_markup("<span weight='bold'>Adding Plugins</span>");
        info_title.set_halign(gtk4::Align::Start);
        info_box.append(&info_title);
        
        let info_text = Label::new(Some("1. Place .wasm files in ~/.config/fox-osc/plugins/\n2. Restart the application\n3. Plugins will load automatically"));
        info_text.set_halign(gtk4::Align::Start);
        info_box.append(&info_text);
        
        vbox.append(&info_box);
        
        vbox.upcast::<Widget>()
    }
    
    pub fn window(&self) -> &ApplicationWindow {
        &self.window
    }
}