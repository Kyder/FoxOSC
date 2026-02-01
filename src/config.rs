use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub osc: OscConfig,
    pub ui: UiConfig,
    #[serde(default)]
    pub plugins: HashMap<String, PluginConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OscConfig {
    pub bind_address: String,
    pub target_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub console_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    #[serde(default)]
    pub settings: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            osc: OscConfig {
                bind_address: "0.0.0.0:9001".to_string(),
                target_address: "127.0.0.1:9000".to_string(),
            },
            ui: UiConfig {
                console_enabled: true,
            },
            plugins: HashMap::new(),
        }
    }
}

impl Config {
    fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Failed to get config directory"))?
            .join("fox-osc");
        
        fs::create_dir_all(&config_dir)?;
        Ok(config_dir.join("config.toml"))
    }
    
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        let content = fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
    
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }
    
    pub fn load_or_default() -> Result<Self> {
        match Self::load() {
            Ok(config) => Ok(config),
            Err(_) => {
                let config = Self::default();
                config.save()?;
                Ok(config)
            }
        }
    }
    
    pub fn get_plugin_setting(&self, plugin_name: &str, key: &str) -> Option<String> {
        self.plugins
            .get(plugin_name)
            .and_then(|p| p.settings.get(key))
            .cloned()
    }
    
    pub fn set_plugin_setting(&mut self, plugin_name: &str, key: &str, value: &str) {
        let plugin_config = self.plugins
            .entry(plugin_name.to_string())
            .or_insert_with(|| PluginConfig {
                settings: HashMap::new(),
            });
        
        plugin_config.settings.insert(key.to_string(), value.to_string());
    }
}