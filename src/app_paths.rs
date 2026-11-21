use std::env;
use std::path::PathBuf;

const APPLICATION_DIRECTORY: &str = "replay-library";

fn environment_directory(variable: &str) -> PathBuf {
    env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
}

pub fn settings_file() -> PathBuf {
    environment_directory("APPDATA")
        .join(APPLICATION_DIRECTORY)
        .join("settings.toml")
}

pub fn cache_directory() -> PathBuf {
    environment_directory("LOCALAPPDATA").join(APPLICATION_DIRECTORY)
}

pub fn thumbnail_directory() -> PathBuf {
    cache_directory().join("thumbnails")
}

pub fn probe_cache_file() -> PathBuf {
    cache_directory().join("probes.toml")
}
