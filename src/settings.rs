use std::fs;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
    pub server: String,
    #[serde(rename = "directory")]
    pub music_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pref_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

pub fn load() -> Result<Settings, String> {
    load_from("settings.toml")
}

pub fn load_from(path: &str) -> Result<Settings, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("failed to read {path}: {e}"))?;
    toml::from_str(&content).map_err(|e| format!("failed to parse {path}: {e}"))
}

pub fn save(settings: &Settings) -> Result<(), String> {
    save_to("settings.toml", settings)
}

pub fn save_to(path: &str, settings: &Settings) -> Result<(), String> {
    let content = toml::to_string_pretty(settings)
        .map_err(|e| format!("failed to serialize settings: {e}"))?;
    fs::write(path, content).map_err(|e| format!("failed to write {path}: {e}"))
}
