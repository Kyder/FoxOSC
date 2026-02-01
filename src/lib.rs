pub mod config;
pub mod console;
pub mod osc_manager;
pub mod plugin_api;
pub mod wasm_loader;
pub mod ui;

use anyhow::Result;
use std::sync::Arc;
use parking_lot::RwLock;

pub use console::ConsoleLog;
pub use config::Config;
pub use wasm_loader::{WasmPluginLoader, WasmPlugin};

/// Main application state
pub struct AppState {
    pub config: Arc<RwLock<Config>>,
    pub console: Arc<RwLock<ConsoleLog>>,
    pub plugin_loader: Arc<RwLock<WasmPluginLoader>>,
}

impl AppState {
    pub fn new() -> Result<Self> {
        let config = Config::load_or_default()?;
        
        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            console: Arc::new(RwLock::new(ConsoleLog::new())),
            plugin_loader: Arc::new(RwLock::new(WasmPluginLoader::new()?)),
        })
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new().unwrap()
    }
}
