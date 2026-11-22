use std::collections::HashMap;
use std::fs;

use anyhow::{Context as _, Result};

use crate::app_paths;
use crate::library::ClipInfo;

pub type ProbeEntries = HashMap<String, ClipInfo>;

pub fn load() -> ProbeEntries {
    fs::read_to_string(app_paths::probe_cache_file())
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(entries: &ProbeEntries) -> Result<()> {
    let file = app_paths::probe_cache_file();
    fs::create_dir_all(app_paths::cache_directory())
        .context("failed to create the cache directory")?;
    let text = toml::to_string(entries).context("failed to serialize the probe cache")?;
    let temporary = file.with_extension("toml.tmp");
    fs::write(&temporary, text).context("failed to write the probe cache")?;
    fs::rename(&temporary, &file).context("failed to replace the probe cache")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip_through_toml() {
        let mut entries = ProbeEntries::new();
        entries.insert(
            "00ab".into(),
            ClipInfo {
                width: 2560,
                height: 1440,
                frame_rate: 59.94,
                duration_seconds: 42.5,
                codec: "HEVC".into(),
                bitrate: 48_000_000,
                audio_codec: "Opus".into(),
                audio_kbps: 96,
            },
        );
        let text = toml::to_string(&entries).unwrap();
        assert_eq!(toml::from_str::<ProbeEntries>(&text).unwrap(), entries);
    }
}
