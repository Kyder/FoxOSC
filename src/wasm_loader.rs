use anyhow::{Context, Result};
use wasmtime::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::{RwLock, Mutex};
use std::fs;
use chrono::{Local, Timelike};
use rosc::OscType;

use crate::plugin_api::{PluginInfo, UiConfig};
use crate::console::ConsoleLog;
use crate::osc_manager::OscManager;
use crate::config::Config;

pub struct WasmPlugin {
    name: String,
    instance: Arc<Mutex<Instance>>,
    store: Arc<Mutex<Store<PluginState>>>,
    info: PluginInfo,
    ui_config: Option<UiConfig>,
    running: Arc<RwLock<bool>>,
    app_config: Arc<RwLock<Config>>,
}

#[derive(Clone)]
pub struct PluginState {
    pub osc_manager: Arc<OscManager>,
    pub console: Arc<RwLock<ConsoleLog>>,
    pub app_config: Arc<RwLock<Config>>,
    pub plugin_name: String,
}

impl WasmPlugin {
    pub fn new(
        path: &Path,
        osc_manager: Arc<OscManager>,
        console: Arc<RwLock<ConsoleLog>>,
        app_config: Arc<RwLock<Config>>,
    ) -> Result<Self> {
        // Create WASM engine
        let engine = Engine::default();
        
        // Read WASM module
        let module = Module::from_file(&engine, path)
            .context("Failed to load WASM module")?;
        
        // Create linker with host functions
        let mut linker = Linker::new(&engine);
        
        // Add host functions that plugins can call
        Self::add_host_functions(&mut linker)?;
        
        // Get plugin info first (need it for state)
        let mut temp_store = Store::new(&engine, PluginState {
            osc_manager: osc_manager.clone(),
            console: console.clone(),
            app_config: app_config.clone(),
            plugin_name: "temp".to_string(),
        });
        
        let temp_instance = linker.instantiate(&mut temp_store, &module)
            .context("Failed to instantiate WASM module")?;
        
        let info = Self::call_get_info(&temp_instance, &mut temp_store)?;
        let name = info.name.clone();
        
        // Now create proper store with correct plugin name
        let state = PluginState {
            osc_manager: osc_manager.clone(),
            console: console.clone(),
            app_config: app_config.clone(),
            plugin_name: name.clone(),
        };
        let mut store = Store::new(&engine, state);
        
        // Instantiate again with proper state
        let instance = linker.instantiate(&mut store, &module)
            .context("Failed to instantiate WASM module")?;
        
        // Try to get UI config
        let ui_config = Self::call_get_ui_config(&instance, &mut store).ok();
        
        console.write().log_info(&format!("Loaded plugin: {} v{}", info.name, info.version));
        
        Ok(Self {
            name,
            instance: Arc::new(Mutex::new(instance)),
            store: Arc::new(Mutex::new(store)),
            info,
            ui_config,
            running: Arc::new(RwLock::new(false)),
            app_config,
        })
    }
    
    fn add_host_functions(linker: &mut Linker<PluginState>) -> Result<()> {
        // get_system_time() -> returns packed u32 with hours, minutes, seconds
        linker.func_wrap(
            "env",
            "get_system_time",
            |_caller: Caller<'_, PluginState>| -> u32 {
                let now = Local::now();
                let hour = now.hour();
                let minute = now.minute();
                let second = now.second();
                
                // Pack into single u32: (hour << 16) | (minute << 8) | second
                ((hour as u32) << 16) | ((minute as u32) << 8) | (second as u32)
            },
        )?;
        
        // get_unix_timestamp() -> returns current Unix timestamp (seconds since epoch)
        linker.func_wrap(
            "env",
            "get_unix_timestamp",
            |_caller: Caller<'_, PluginState>| -> u64 {
                use std::time::{SystemTime, UNIX_EPOCH};
                
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            },
        )?;
        
        // load_config(key_ptr, key_len) -> returns value_ptr or 0 if not found
        linker.func_wrap(
            "env",
            "load_config",
            |mut caller: Caller<'_, PluginState>, key_ptr: i32, key_len: i32| -> i32 {
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(mem) => mem,
                    None => return 0,
                };
                
                let data = memory.data(&caller);
                let key_bytes = &data[key_ptr as usize..(key_ptr + key_len) as usize];
                let key = String::from_utf8_lossy(key_bytes).to_string();
                
                let state = caller.data();
                let config = state.app_config.read();
                
                if let Some(value) = config.get_plugin_setting(&state.plugin_name, &key) {
                    // Write value to a fixed memory location
                    let value_bytes = value.as_bytes();
                    let write_pos = 2048; // Fixed position for config values
                    
                    drop(config);
                    let data = memory.data_mut(&mut caller);
                    
                    if write_pos + 4 + value_bytes.len() < data.len() {
                        // Write length
                        let len = value_bytes.len() as u32;
                        data[write_pos..write_pos + 4].copy_from_slice(&len.to_le_bytes());
                        // Write value
                        data[write_pos + 4..write_pos + 4 + value_bytes.len()].copy_from_slice(value_bytes);
                        return write_pos as i32;
                    }
                }
                
                0
            },
        )?;
        
        // save_config(key_ptr, key_len, value_ptr, value_len)
        linker.func_wrap(
            "env",
            "save_config",
            |mut caller: Caller<'_, PluginState>, key_ptr: i32, key_len: i32, value_ptr: i32, value_len: i32| {
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(mem) => mem,
                    None => return,
                };
                
                let data = memory.data(&caller);
                let key_bytes = &data[key_ptr as usize..(key_ptr + key_len) as usize];
                let key = String::from_utf8_lossy(key_bytes).to_string();
                
                let value_bytes = &data[value_ptr as usize..(value_ptr + value_len) as usize];
                let value = String::from_utf8_lossy(value_bytes).to_string();
                
                let state = caller.data();
                let mut config = state.app_config.write();
                config.set_plugin_setting(&state.plugin_name, &key, &value);
                
                // Save to disk
                if let Err(e) = config.save() {
                    state.console.write().log_error(&format!("Failed to save config: {}", e));
                }
            },
        )?;
        
        // osc_send_float(address_ptr, address_len, value)
        linker.func_wrap(
            "env",
            "osc_send_float",
            |mut caller: Caller<'_, PluginState>, addr_ptr: i32, addr_len: i32, value: f32| -> i32 {
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(mem) => mem,
                    None => return 0,
                };
                
                let data = memory.data(&caller);
                let addr_bytes = &data[addr_ptr as usize..(addr_ptr + addr_len) as usize];
                let address = String::from_utf8_lossy(addr_bytes).to_string();
                
                let state = caller.data();
                if let Err(e) = state.osc_manager.send_float(&address, value) {
                    state.console.write().log_error(&format!("OSC send failed: {}", e));
                    return 0;
                }
                
                1
            },
        )?;
        
        // osc_send_chatbox(message_ptr, message_len, typing)
        linker.func_wrap(
            "env",
            "osc_send_chatbox",
            |mut caller: Caller<'_, PluginState>, msg_ptr: i32, msg_len: i32, typing: i32| -> i32 {
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(mem) => mem,
                    None => return 0,
                };
                
                let data = memory.data(&caller);
                let msg_bytes = &data[msg_ptr as usize..(msg_ptr + msg_len) as usize];
                let message = String::from_utf8_lossy(msg_bytes).to_string();
                
                let state = caller.data();
                // typing != 0 means open keyboard, typing == 0 means send immediately
                if let Err(e) = state.osc_manager.send_chatbox(&message, typing != 0) {
                    state.console.write().log_error(&format!("OSC chatbox send failed: {}", e));
                    return 0;
                }
                
                1
            },
        )?;
        
        // log_info(msg_ptr, msg_len)
        linker.func_wrap(
            "env",
            "log_info",
            |mut caller: Caller<'_, PluginState>, msg_ptr: i32, msg_len: i32| {
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(mem) => mem,
                    None => return,
                };
                
                let data = memory.data(&caller);
                let msg_bytes = &data[msg_ptr as usize..(msg_ptr + msg_len) as usize];
                let message = String::from_utf8_lossy(msg_bytes).to_string();
                
                let state = caller.data();
                state.console.write().log_info(&message);
            },
        )?;
        
        // log_error(msg_ptr, msg_len)
        linker.func_wrap(
            "env",
            "log_error",
            |mut caller: Caller<'_, PluginState>, msg_ptr: i32, msg_len: i32| {
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(mem) => mem,
                    None => return,
                };
                
                let data = memory.data(&caller);
                let msg_bytes = &data[msg_ptr as usize..(msg_ptr + msg_len) as usize];
                let message = String::from_utf8_lossy(msg_bytes).to_string();
                
                let state = caller.data();
                state.console.write().log_error(&message);
            },
        )?;
        
        Ok(())
    }
    
    pub fn register_osc_boop_listener(&self) -> Result<()> {
        // Get the configured boop address
        let config = self.app_config.read();
        let boop_addr = config
            .get_plugin_setting(&self.name, "boop_input_address")
            .unwrap_or_else(|| "/avatar/parameters/OSCBoop".to_string());
        drop(config);
        
        // Register listener with callback to plugin
        let instance = self.instance.clone();
        let store = self.store.clone();
        let console = self.store.lock().data().console.clone();
        
        self.store.lock().data().osc_manager.register_listener(
            boop_addr.clone(),
            move |_addr, value| {
                // Call plugin_on_osc_bool when we receive the bool
                match value {
                    OscType::Bool(b) => {
                        let inst = instance.lock();
                        let mut st = store.lock();
                        
                        if let Ok(callback_fn) = inst.get_typed_func::<i32, ()>(&mut *st, "plugin_on_osc_bool") {
                            let val = if *b { 1 } else { 0 };
                            if let Err(e) = callback_fn.call(&mut *st, val) {
                                console.write().log_error(&format!("Failed to call plugin_on_osc_bool: {}", e));
                            }
                        }
                    }
                    OscType::Float(f) => {
                        // Treat as bool: non-zero = true
                        let inst = instance.lock();
                        let mut st = store.lock();
                        
                        if let Ok(callback_fn) = inst.get_typed_func::<i32, ()>(&mut *st, "plugin_on_osc_bool") {
                            let val = if *f > 0.5 { 1 } else { 0 };
                            if let Err(e) = callback_fn.call(&mut *st, val) {
                                console.write().log_error(&format!("Failed to call plugin_on_osc_bool: {}", e));
                            }
                        }
                    }
                    _ => {}
                }
            },
        );
        
        Ok(())
    }
    
    fn read_string_from_memory(memory: &Memory, store: &Store<PluginState>, ptr: i32) -> Result<String> {
        let data = memory.data(&store);
        
        // First 4 bytes = length
        let len_bytes = &data[ptr as usize..ptr as usize + 4];
        let len = u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]) as usize;
        
        // Next len bytes = data
        let str_bytes = &data[ptr as usize + 4..ptr as usize + 4 + len];
        let string = String::from_utf8_lossy(str_bytes).to_string();
        
        Ok(string)
    }
    
    fn call_get_info(instance: &Instance, store: &mut Store<PluginState>) -> Result<PluginInfo> {
        let get_info = instance.get_typed_func::<(), i32>(&mut *store, "plugin_info")
            .context("Plugin missing plugin_info function")?;
        
        let ptr = get_info.call(&mut *store, ())
            .context("Failed to call plugin_info")?;
        
        let memory = instance.get_memory(&mut *store, "memory")
            .context("Plugin missing memory export")?;
        
        let json = Self::read_string_from_memory(&memory, store, ptr)?;
        
        let info: PluginInfo = serde_json::from_str(&json)
            .context("Failed to parse plugin info JSON")?;
        
        Ok(info)
    }
    
    fn call_get_ui_config(instance: &Instance, store: &mut Store<PluginState>) -> Result<UiConfig> {
        let get_ui = instance.get_typed_func::<(), i32>(&mut *store, "plugin_ui_config")
            .context("Plugin missing plugin_ui_config function")?;
        
        let ptr = get_ui.call(&mut *store, ())
            .context("Failed to call plugin_ui_config")?;
        
        let memory = instance.get_memory(&mut *store, "memory")
            .context("Plugin missing memory export")?;
        
        let json = Self::read_string_from_memory(&memory, store, ptr)?;
        
        let ui_config: UiConfig = serde_json::from_str(&json)
            .context("Failed to parse UI config JSON")?;
        
        Ok(ui_config)
    }
    
    pub fn load_config_from_disk(&mut self) -> Result<()> {
        let inst = self.instance.lock();
        let mut store = self.store.lock();
        
        // Call plugin_load_config if it exists
        if let Ok(load_fn) = inst.get_typed_func::<(), ()>(&mut *store, "plugin_load_config") {
            load_fn.call(&mut *store, ())?;
        }
        Ok(())
    }
    
    pub fn send_ui_event(&mut self, event_json: &str) -> Result<()> {
        let inst = self.instance.lock();
        let mut store = self.store.lock();
        
        // Call plugin_ui_event if it exists
        if let Ok(ui_event_fn) = inst.get_typed_func::<(i32, i32), ()>(&mut *store, "plugin_ui_event") {
            let bytes = event_json.as_bytes();
            
            // Allocate memory in WASM for the event JSON
            let memory = inst.get_memory(&mut *store, "memory")
                .context("Plugin missing memory export")?;
            
            let data = memory.data_mut(&mut *store);
            let write_pos = 1024; // Fixed position for event data
            
            if write_pos + bytes.len() < data.len() {
                data[write_pos..write_pos + bytes.len()].copy_from_slice(bytes);
                
                ui_event_fn.call(&mut *store, (write_pos as i32, bytes.len() as i32))?;
            }
        }
        
        Ok(())
    }
    
    pub fn info(&self) -> &PluginInfo {
        &self.info
    }
    
    pub fn ui_config(&self) -> Option<&UiConfig> {
        self.ui_config.as_ref()
    }
    
    pub fn start(&mut self) -> Result<()> {
        if *self.running.read() {
            return Ok(());
        }
        
        let inst = self.instance.lock();
        let mut store = self.store.lock();
        
        let start_fn = inst.get_typed_func::<(), ()>(&mut *store, "plugin_start")
            .context("Plugin missing plugin_start function")?;
        
        start_fn.call(&mut *store, ())
            .context("Failed to call plugin_start")?;
        
        *self.running.write() = true;
        store.data().console.write().log_info(&format!("Started plugin: {}", self.name));
        
        Ok(())
    }
    
    pub fn stop(&mut self) -> Result<()> {
        if !*self.running.read() {
            return Ok(());
        }
        
        let inst = self.instance.lock();
        let mut store = self.store.lock();
        
        let stop_fn = inst.get_typed_func::<(), ()>(&mut *store, "plugin_stop")
            .context("Plugin missing plugin_stop function")?;
        
        stop_fn.call(&mut *store, ())
            .context("Failed to call plugin_stop")?;
        
        *self.running.write() = false;
        store.data().console.write().log_info(&format!("Stopped plugin: {}", self.name));
        
        Ok(())
    }
    
    pub fn update(&mut self) -> Result<()> {
        if !*self.running.read() {
            return Ok(());
        }
        
        let inst = self.instance.lock();
        let mut store = self.store.lock();
        
        // Call plugin_update if it exists
        if let Ok(update_fn) = inst.get_typed_func::<(), ()>(&mut *store, "plugin_update") {
            update_fn.call(&mut *store, ())?;
        }
        
        Ok(())
    }
    
    pub fn is_running(&self) -> bool {
        *self.running.read()
    }
}

pub struct WasmPluginLoader {
    plugins_dir: PathBuf,
    plugins: Vec<WasmPlugin>,
}

impl WasmPluginLoader {
    pub fn new() -> Result<Self> {
        let plugins_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Failed to get config directory"))?
            .join("fox-osc")
            .join("plugins");
        
        fs::create_dir_all(&plugins_dir)?;
        
        Ok(Self {
            plugins_dir,
            plugins: Vec::new(),
        })
    }
    
    pub fn load_all(
        &mut self,
        osc_manager: Arc<OscManager>,
        console: Arc<RwLock<ConsoleLog>>,
        app_config: Arc<RwLock<Config>>,
    ) -> Result<()> {
        console.write().log_info(&format!("Loading plugins from: {}", self.plugins_dir.display()));
        
        // Find all .wasm files
        let entries = fs::read_dir(&self.plugins_dir)?;
        
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                match WasmPlugin::new(&path, osc_manager.clone(), console.clone(), app_config.clone()) {
                    Ok(mut plugin) => {
                        console.write().log_info(&format!("✔ Loaded: {}", plugin.info().name));
                        
                        // Load config from disk
                        if let Err(e) = plugin.load_config_from_disk() {
                            console.write().log_error(&format!("Failed to load config for {}: {}", plugin.info().name, e));
                        }
                        
                        // Register OSC listener for Boop Counter
                        if plugin.info().name == "Boop Counter" {
                            if let Err(e) = plugin.register_osc_boop_listener() {
                                console.write().log_error(&format!("Failed to register OSC listener for {}: {}", plugin.info().name, e));
                            }
                        }
                        
                        self.plugins.push(plugin);
                    }
                    Err(e) => {
                        console.write().log_error(&format!("âœ— Failed to load {}: {}", path.display(), e));
                    }
                }
            }
        }
        
        console.write().log_info(&format!("Loaded {} plugin(s)", self.plugins.len()));
        
        Ok(())
    }
    
    pub fn plugins(&self) -> &[WasmPlugin] {
        &self.plugins
    }
    
    pub fn plugins_mut(&mut self) -> &mut [WasmPlugin] {
        &mut self.plugins
    }
    
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }
}