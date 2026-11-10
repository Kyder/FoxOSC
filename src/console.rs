//! Console log storage. Pure data, no UI code — the egui front end in
//! `ui::console_page` reads from this every frame.

use std::collections::{HashMap, VecDeque};

#[derive(Clone, Debug)]
pub enum LogEntry {
    Info(String),
    Error(String),
    OscSent { address: String, value: String },
    OscReceived { address: String, value: String },
}

#[derive(Clone, Debug)]
pub struct TimedEntry {
    /// Local wall-clock time, "HH:MM:SS"
    pub time: String,
    pub entry: LogEntry,
}

pub struct ConsoleLog {
    enabled: bool,
    entries: VecDeque<TimedEntry>,
    max_entries: usize,
    active_addresses: HashMap<String, String>, // address -> current value
}

impl Default for ConsoleLog {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleLog {
    pub fn new() -> Self {
        Self {
            enabled: true,
            entries: VecDeque::new(),
            max_entries: 1000,
            active_addresses: HashMap::new(),
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn push(&mut self, entry: LogEntry) {
        if !self.enabled {
            return;
        }
        self.entries.push_back(TimedEntry {
            time: chrono::Local::now().format("%H:%M:%S").to_string(),
            entry,
        });
        while self.entries.len() > self.max_entries {
            self.entries.pop_front();
        }
    }

    pub fn log_info(&mut self, message: &str) {
        self.push(LogEntry::Info(message.to_string()));
    }

    pub fn log_error(&mut self, message: &str) {
        self.push(LogEntry::Error(message.to_string()));
    }

    pub fn log_osc_sent(&mut self, address: &str, value: &str) {
        self.push(LogEntry::OscSent {
            address: address.to_string(),
            value: value.to_string(),
        });
    }

    pub fn log_osc_received(&mut self, address: &str, value: &str) {
        if !self.enabled {
            return;
        }
        self.active_addresses.insert(address.to_string(), value.to_string());
        self.push(LogEntry::OscReceived {
            address: address.to_string(),
            value: value.to_string(),
        });
    }

    /// Update active address without logging to entries (for unlistened addresses)
    pub fn update_active_address(&mut self, address: &str, value: &str) {
        self.active_addresses.insert(address.to_string(), value.to_string());
    }

    pub fn entries(&self) -> &VecDeque<TimedEntry> {
        &self.entries
    }

    pub fn get_active_addresses(&self) -> &HashMap<String, String> {
        &self.active_addresses
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn clear_active_addresses(&mut self) {
        self.active_addresses.clear();
    }
}
