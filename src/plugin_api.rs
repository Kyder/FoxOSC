use serde::{Deserialize, Serialize};

/// Information about a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
}

/// UI configuration element types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiElement {
    TextInput {
        id: String,
        label: String,
        default_value: String,
        placeholder: String,
    },
    Button {
        id: String,
        label: String,
    },
    Label {
        text: String,
    },
    Separator,
    /// A live, read-only value shown as a stat tile. The host reads
    /// `config_key` from this plugin's saved settings and refreshes it
    /// continuously, so a plugin only has to `save_config` to update it.
    ///
    /// New in 0.2 — older plugins that don't send it are unaffected.
    LiveValue {
        label: String,
        config_key: String,
    },
    /// On/off switch. `value` is the current state as the plugin sees it.
    /// Flipping it sends `UiEvent::ToggleChanged` immediately (no Apply needed).
    Toggle {
        id: String,
        label: String,
        value: bool,
    },
    /// A table with evenly sized, centred columns and an optional button per
    /// row. A row button click arrives as an ordinary `ButtonClicked` event.
    Table {
        columns: Vec<String>,
        rows: Vec<TableRow>,
        /// Shown instead of the rows when there are none.
        #[serde(default)]
        empty_text: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRow {
    pub cells: Vec<String>,
    #[serde(default)]
    pub button: Option<TableButton>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableButton {
    pub id: String,
    pub label: String,
}

/// UI configuration that plugins can provide
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub title: String,
    pub elements: Vec<UiElement>,
}

/// Events from UI to plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiEvent {
    ButtonClicked { id: String },
    TextChanged { id: String, value: String },
    ApplySettings { values: Vec<(String, String)> },
    /// JSON: {"ToggleChanged":{"id":"quiet_mode","value":true}}
    ToggleChanged { id: String, value: bool },
}
