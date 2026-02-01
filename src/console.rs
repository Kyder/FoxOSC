use gtk4::prelude::*;
use gtk4::{TextView, ScrolledWindow, Box as GtkBox, Orientation, Notebook, Label, Switch, Paned, Widget};
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum LogEntry {
    Info(String),
    Error(String),
    OscSent { address: String, value: String },
    OscReceived { address: String, value: String },
}

pub struct ConsoleLog {
    enabled: bool,
    entries: Vec<LogEntry>,
    max_entries: usize,
    active_addresses: HashMap<String, String>, // address -> current value
    last_displayed_count: usize, // Track how many entries we've displayed
}

impl ConsoleLog {
    pub fn new() -> Self {
        Self {
            enabled: true,
            entries: Vec::new(),
            max_entries: 1000,
            active_addresses: HashMap::new(),
            last_displayed_count: 0,
        }
    }
    
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    pub fn log_info(&mut self, message: &str) {
        if !self.enabled {
            return;
        }
        
        self.entries.push(LogEntry::Info(message.to_string()));
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }
    
    pub fn log_error(&mut self, message: &str) {
        if !self.enabled {
            return;
        }
        
        self.entries.push(LogEntry::Error(message.to_string()));
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }
    
    pub fn log_osc_sent(&mut self, address: &str, value: &str) {
        if !self.enabled {
            return;
        }
        
        self.entries.push(LogEntry::OscSent {
            address: address.to_string(),
            value: value.to_string(),
        });
        
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }
    
    pub fn log_osc_received(&mut self, address: &str, value: &str) {
        if !self.enabled {
            return;
        }
        
        // Update active addresses
        self.active_addresses.insert(address.to_string(), value.to_string());
        
        self.entries.push(LogEntry::OscReceived {
            address: address.to_string(),
            value: value.to_string(),
        });
        
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }
    
    // Update active address without logging to entries (for unlistened addresses)
    pub fn update_active_address(&mut self, address: &str, value: &str) {
        // Only update active addresses map, don't add to log entries
        self.active_addresses.insert(address.to_string(), value.to_string());
    }
    
    pub fn get_entries(&self) -> &[LogEntry] {
        &self.entries
    }
    
    pub fn get_new_entries(&mut self) -> &[LogEntry] {
        let new_entries = &self.entries[self.last_displayed_count..];
        self.last_displayed_count = self.entries.len();
        new_entries
    }
    
    pub fn reset_display_count(&mut self) {
        self.last_displayed_count = 0;
    }
    
    pub fn get_active_addresses(&self) -> &HashMap<String, String> {
        &self.active_addresses
    }
    
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

pub struct ConsoleViews {
    pub unified_view: TextView,
    pub sent_view: TextView,
    pub received_view: TextView,
    pub active_view: TextView,
}

pub fn create_console_ui(console: Arc<RwLock<ConsoleLog>>) -> (GtkBox, Switch, ConsoleViews) {
    let vbox = GtkBox::new(Orientation::Vertical, 5);
    vbox.set_margin_top(10);
    vbox.set_margin_bottom(10);
    vbox.set_margin_start(10);
    vbox.set_margin_end(10);
    
    // Console enable/disable switch at top
    let header_box = GtkBox::new(Orientation::Horizontal, 10);
    let console_label = Label::new(Some("Console Enabled:"));
    let console_switch = Switch::new();
    console_switch.set_active(console.read().is_enabled());
    header_box.append(&console_label);
    header_box.append(&console_switch);
    vbox.append(&header_box);
    
    // Notebook for tabs
    let notebook = Notebook::new();
    
    // Tab 1: Log with sorting
    let (log_tab, sort_switch, unified_view, sent_view, received_view) = create_log_tab();
    notebook.append_page(&log_tab, Some(&Label::new(Some("Log"))));
    
    // Tab 2: Active Addresses
    let (active_tab, active_view) = create_active_addresses_tab();
    notebook.append_page(&active_tab, Some(&Label::new(Some("Active Addresses"))));
    
    vbox.append(&notebook);
    
    let views = ConsoleViews {
        unified_view: unified_view.clone(),
        sent_view: sent_view.clone(),
        received_view: received_view.clone(),
        active_view: active_view.clone(),
    };
    
    // Setup update timers
    let console_clone = console.clone();
    let unified_clone = unified_view.clone();
    let sent_clone = sent_view.clone();
    let received_clone = received_view.clone();
    let sort_clone = sort_switch.clone();
    
    glib::timeout_add_seconds_local(1, move || {
        update_log_view(&console_clone, &unified_clone, &sent_clone, &received_clone, sort_clone.is_active());
        glib::ControlFlow::Continue
    });
    
    let console_clone2 = console.clone();
    let active_clone = active_view.clone();
    glib::timeout_add_seconds_local(1, move || {
        update_active_addresses_view(&console_clone2, &active_clone);
        glib::ControlFlow::Continue
    });
    
    (vbox, console_switch, views)
}

fn create_log_tab() -> (GtkBox, Switch, TextView, TextView, TextView) {
    let vbox = GtkBox::new(Orientation::Vertical, 5);
    
    // Sort switch
    let sort_box = GtkBox::new(Orientation::Horizontal, 10);
    let sort_label = Label::new(Some("Split Sent/Received:"));
    let sort_switch = Switch::new();
    sort_switch.set_active(false);
    sort_box.append(&sort_label);
    sort_box.append(&sort_switch);
    sort_box.set_margin_bottom(5);
    vbox.append(&sort_box);
    
    // Paned view for split mode
    let paned = Paned::new(Orientation::Horizontal);
    
    // Left: Sent
    let sent_scroll = ScrolledWindow::new();
    sent_scroll.set_vexpand(true);
    let sent_view = TextView::new();
    sent_view.set_editable(false);
    sent_view.set_monospace(true);
    sent_scroll.set_child(Some(&sent_view));
    
    // Right: Received
    let received_scroll = ScrolledWindow::new();
    received_scroll.set_vexpand(true);
    let received_view = TextView::new();
    received_view.set_editable(false);
    received_view.set_monospace(true);
    received_scroll.set_child(Some(&received_view));
    
    paned.set_start_child(Some(&sent_scroll));
    paned.set_end_child(Some(&received_scroll));
    paned.set_position(400);
    
    // Single unified view for unsorted mode
    let unified_scroll = ScrolledWindow::new();
    unified_scroll.set_vexpand(true);
    let unified_view = TextView::new();
    unified_view.set_editable(false);
    unified_view.set_monospace(true);
    unified_scroll.set_child(Some(&unified_view));
    
    vbox.append(&unified_scroll);
    vbox.append(&paned);
    
    // Initially show unified, hide paned
    unified_scroll.set_visible(true);
    paned.set_visible(false);
    
    // Switch handler
    let unified_clone = unified_scroll.clone();
    let paned_clone = paned.clone();
    sort_switch.connect_state_set(move |_, sorted| {
        if sorted {
            unified_clone.set_visible(false);
            paned_clone.set_visible(true);
        } else {
            unified_clone.set_visible(true);
            paned_clone.set_visible(false);
        }
        glib::Propagation::Proceed
    });
    
    (vbox, sort_switch, unified_view, sent_view, received_view)
}

fn create_active_addresses_tab() -> (ScrolledWindow, TextView) {
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    
    let text_view = TextView::new();
    text_view.set_editable(false);
    text_view.set_monospace(true);
    
    scroll.set_child(Some(&text_view));
    (scroll, text_view)
}

fn update_log_view(console: &Arc<RwLock<ConsoleLog>>, unified_view: &TextView, sent_view: &TextView, received_view: &TextView, sorted: bool) {
    let new_entries = {
        let mut console_lock = console.write();
        console_lock.get_new_entries().to_vec()
    };
    
    // If no new entries, nothing to do
    if new_entries.is_empty() {
        return;
    }
    
    if sorted {
        // Split mode - append to appropriate buffers
        let mut sent_text = String::new();
        let mut received_text = String::new();
        
        for entry in &new_entries {
            match entry {
                LogEntry::OscSent { address, value } => {
                    sent_text.push_str(&format!("→ {} = {}\n", address, value));
                }
                LogEntry::OscReceived { address, value } => {
                    received_text.push_str(&format!("← {} = {}\n", address, value));
                }
                LogEntry::Info(msg) => {
                    sent_text.push_str(&format!("ℹ {}\n", msg));
                }
                LogEntry::Error(msg) => {
                    sent_text.push_str(&format!("✗ {}\n", msg));
                }
            }
        }
        
        // Append new text without clearing (no flicker!)
        if !sent_text.is_empty() {
            append_text_with_smart_scroll(sent_view, &sent_text);
        }
        if !received_text.is_empty() {
            append_text_with_smart_scroll(received_view, &received_text);
        }
    } else {
        // Unified mode - append all new entries
        let mut text = String::new();
        
        for entry in &new_entries {
            match entry {
                LogEntry::Info(msg) => text.push_str(&format!("ℹ {}\n", msg)),
                LogEntry::Error(msg) => text.push_str(&format!("✗ {}\n", msg)),
                LogEntry::OscSent { address, value } => {
                    text.push_str(&format!("→ {} = {}\n", address, value));
                }
                LogEntry::OscReceived { address, value } => {
                    text.push_str(&format!("← {} = {}\n", address, value));
                }
            }
        }
        
        // Append new text
        append_text_with_smart_scroll(unified_view, &text);
    }
}

// Append text to TextView with smart scrolling (only auto-scroll if at bottom)
fn append_text_with_smart_scroll(text_view: &TextView, text: &str) {
    // Find the ScrolledWindow parent
    let mut current = text_view.clone().upcast::<Widget>();
    let mut scrolled_window: Option<ScrolledWindow> = None;
    
    while let Some(parent) = current.parent() {
        if let Some(sw) = parent.downcast_ref::<ScrolledWindow>() {
            scrolled_window = Some(sw.clone());
            break;
        }
        current = parent;
    }
    
    let should_auto_scroll = if let Some(sw) = &scrolled_window {
        let vadj = sw.vadjustment();
        let value = vadj.value();
        let upper = vadj.upper();
        let page_size = vadj.page_size();
        
        // Consider "at bottom" if within 50 pixels of the bottom
        (value + page_size) >= (upper - 50.0)
    } else {
        false
    };
    
    // Append the text to the end of buffer (no clearing!)
    let buffer = text_view.buffer();
    let mut end_iter = buffer.end_iter();
    buffer.insert(&mut end_iter, text);
    
    // If we were at bottom, scroll to new bottom
    if should_auto_scroll {
        let text_view_clone = text_view.clone();
        glib::idle_add_local_once(move || {
            let buffer = text_view_clone.buffer();
            let end_iter = buffer.end_iter();
            text_view_clone.scroll_to_iter(&mut end_iter.clone(), 0.0, false, 0.0, 0.0);
        });
    }
    // If NOT at bottom, do nothing - position stays exactly where it is
}

fn update_active_addresses_view(console: &Arc<RwLock<ConsoleLog>>, view: &TextView) {
    let active = console.read().get_active_addresses().clone();
    
    let mut buffer = String::new();
    buffer.push_str("Active OSC Addresses (live values):\n");
    buffer.push_str("═══════════════════════════════════\n\n");
    
    let mut sorted: Vec<_> = active.iter().collect();
    sorted.sort_by_key(|(addr, _)| *addr);
    
    for (address, value) in sorted {
        buffer.push_str(&format!("{:<50} = {}\n", address, value));
    }
    
    if active.is_empty() {
        buffer.push_str("\n(No OSC addresses received yet)\n");
    }
    
    view.buffer().set_text(&buffer);
}