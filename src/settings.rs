use crate::{i18n::Language, storage};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    pub theme: Theme,
    pub reduced_motion: bool,
    pub startup_updates: bool,
    pub snap: bool,
    pub grid: f64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::English,
            theme: Theme::Light,
            reduced_motion: false,
            startup_updates: true,
            snap: true,
            grid: 0.1,
        }
    }
}
impl Settings {
    pub fn path() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
            .unwrap_or_else(std::env::temp_dir)
            .join("RoomPlanner")
            .join("settings.json")
    }
    pub fn load() -> Result<Self, String> {
        let path = Self::path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let value: Self = storage::read_json(&path, 64 * 1024)?;
        if !value.grid.is_finite() || !(0.01..=1.0).contains(&value.grid) {
            return Err("Invalid grid spacing".into());
        }
        Ok(value)
    }
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        storage::save_json(&path, self)
    }
}
