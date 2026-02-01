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
}