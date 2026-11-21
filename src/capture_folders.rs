use std::path::{Path, PathBuf};

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_ANY, RegGetValueW};
use windows::Win32::UI::Shell::{FOLDERID_Videos, KNOWN_FOLDER_FLAG, SHGetKnownFolderPath};
use windows::core::{GUID, HSTRING};

use crate::settings::{FolderConfig, FolderKind};

const AMD_RECORDING_KEY: &str = "Software\\AMD\\DVR";
const AMD_RECORDING_VALUE: &str = "CaptureFileOutput";
const NVIDIA_RECORDING_KEY: &str = "Software\\NVIDIA Corporation\\Global\\ShadowPlay\\NVSPCAPS";
const NVIDIA_RECORDING_VALUE: &str = "DefaultPathW";
const CAPTURES_FOLDER: GUID = GUID::from_u128(0xEDC0FE71_98D8_4F4A_B920_C8DC133CB165);

pub fn detect() -> Vec<FolderConfig> {
    let videos = known_folder(&FOLDERID_Videos);
    let mut candidates = Vec::new();
    if let Some(path) = registry_path(AMD_RECORDING_KEY, AMD_RECORDING_VALUE) {
        candidates.push((path, FolderKind::Amd));
    }
    if let Some(videos) = &videos {
        candidates.push((videos.join("Radeon ReLive"), FolderKind::Amd));
    }
    if let Some(path) = registry_path(NVIDIA_RECORDING_KEY, NVIDIA_RECORDING_VALUE) {
        candidates.push((path, FolderKind::Nvidia));
    }
    if let Some(videos) = &videos {
        candidates.push((videos.join("NVIDIA"), FolderKind::Nvidia));
    }
    if let Some(captures) = known_folder(&CAPTURES_FOLDER) {
        candidates.push((captures, FolderKind::Gamebar));
    }
    if let Some(videos) = &videos {
        candidates.push((videos.join("Captures"), FolderKind::Gamebar));
        candidates.push((videos.join("Screen Recordings"), FolderKind::Other));
    }
    select_existing(candidates, videos.as_deref())
}

fn select_existing(
    candidates: Vec<(PathBuf, FolderKind)>,
    videos_root: Option<&Path>,
) -> Vec<FolderConfig> {
    let mut folders: Vec<FolderConfig> = Vec::new();
    for (path, kind) in candidates {
        let too_broad = videos_root.is_some_and(|root| same_location(root, &path));
        let duplicate = folders.iter().any(|folder| {
            same_location(&folder.path, &path)
                || path.starts_with(&folder.path)
                || folder.path.starts_with(&path)
        });
        if path.is_dir() && !too_broad && !duplicate {
            folders.push(FolderConfig {
                path,
                enabled: true,
                kind,
            });
        }
    }
    folders
}

fn same_location(first: &Path, second: &Path) -> bool {
    let normalize = |path: &Path| {
        path.to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
            .replace('/', "\\")
    };
    normalize(first) == normalize(second)
}

fn utf16_bytes_to_path(bytes: &[u8]) -> Option<PathBuf> {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .take_while(|unit| *unit != 0)
        .collect();
    let text = String::from_utf16(&units).ok()?;
    (!text.trim().is_empty()).then(|| PathBuf::from(text.trim()))
}

fn registry_path(key: &str, value: &str) -> Option<PathBuf> {
    let key = HSTRING::from(key);
    let value = HSTRING::from(value);
    let mut size = 0u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            &value,
            RRF_RT_ANY,
            None,
            None,
            Some(&mut size),
        )
        .ok()
        .ok()?;
        let mut buffer = vec![0u8; size as usize];
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            &value,
            RRF_RT_ANY,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()
        .ok()?;
        utf16_bytes_to_path(&buffer[..size as usize])
    }
}

fn known_folder(identifier: &GUID) -> Option<PathBuf> {
    unsafe {
        let raw = SHGetKnownFolderPath(identifier, KNOWN_FOLDER_FLAG(0), None).ok()?;
        let path = raw.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(raw.as_ptr().cast()));
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_value_stops_at_the_terminator() {
        let bytes: Vec<u8> = "D:\\Clips"
            .encode_utf16()
            .chain([0, 0x41])
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(
            utf16_bytes_to_path(&bytes),
            Some(PathBuf::from("D:\\Clips"))
        );
        assert_eq!(utf16_bytes_to_path(&[0, 0]), None);
    }

    #[test]
    fn locations_compare_without_case_or_trailing_separator() {
        assert!(same_location(
            Path::new("C:\\Users\\Me\\Videos\\"),
            Path::new("c:/users/me/videos")
        ));
        assert!(!same_location(
            Path::new("C:\\Videos"),
            Path::new("C:\\Videos\\NVIDIA")
        ));
    }

    #[test]
    fn real_folders_are_listed_once_and_the_videos_root_is_skipped() {
        let root = std::env::temp_dir().join("replay-library-detect-test");
        let amd = root.join("Radeon ReLive");
        std::fs::create_dir_all(amd.join("Game")).unwrap();
        let folders = select_existing(
            vec![
                (root.clone(), FolderKind::Nvidia),
                (amd.clone(), FolderKind::Amd),
                (amd.join("Game"), FolderKind::Other),
                (amd.clone(), FolderKind::Amd),
                (root.join("Missing"), FolderKind::Gamebar),
            ],
            Some(&root),
        );
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].kind, FolderKind::Amd);
        std::fs::remove_dir_all(root).unwrap();
    }
}
