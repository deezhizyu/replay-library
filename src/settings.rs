use std::fs;
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use crate::app_paths;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderKind {
    Nvidia,
    Amd,
    Gamebar,
    Other,
}

impl FolderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Nvidia => "NVIDIA",
            Self::Amd => "AMD",
            Self::Gamebar => "Game Bar",
            Self::Other => "Other",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderConfig {
    pub path: PathBuf,
    pub enabled: bool,
    pub kind: FolderKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub folders: Vec<FolderConfig>,
    pub watch_for_new_clips: bool,
    pub hover_scrub_previews: bool,
    pub thumbnail_size: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            folders: Vec::new(),
            watch_for_new_clips: true,
            hover_scrub_previews: true,
            thumbnail_size: 0.45,
        }
    }
}

impl gpui::Global for Settings {}

impl Settings {
    pub fn load() -> Option<Self> {
        let text = fs::read_to_string(app_paths::settings_file()).ok()?;
        toml::from_str::<Self>(&text).ok().map(Self::clamped)
    }

    pub fn save(&self) -> Result<()> {
        let file = app_paths::settings_file();
        let directory = file
            .parent()
            .context("settings file has no parent directory")?;
        fs::create_dir_all(directory).context("failed to create the settings directory")?;
        let text = toml::to_string_pretty(self).context("failed to serialize the settings")?;
        let temporary = file.with_extension("toml.tmp");
        fs::write(&temporary, text).context("failed to write the settings")?;
        fs::rename(&temporary, &file).context("failed to replace the settings file")
    }

    fn clamped(mut self) -> Self {
        self.thumbnail_size = self.thumbnail_size.clamp(0.0, 1.0);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let settings = Settings {
            folders: vec![FolderConfig {
                path: PathBuf::from("D:/Captures/AMD"),
                enabled: false,
                kind: FolderKind::Amd,
            }],
            thumbnail_size: 0.7,
            ..Settings::default()
        };
        let text = toml::to_string_pretty(&settings).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), settings);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults_and_size_is_clamped() {
        let settings = toml::from_str::<Settings>("thumbnail_size = 4.0")
            .unwrap()
            .clamped();
        assert_eq!(settings.thumbnail_size, 1.0);
        assert!(settings.watch_for_new_clips);
    }
}
