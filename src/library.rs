use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::app_paths;
use crate::clip_name;
use crate::format::format_size;
use crate::local_time::{Date, LocalDateTime, days_between};
use crate::settings::FolderConfig;

const VIDEO_EXTENSIONS: [&str; 5] = ["mp4", "mkv", "mov", "webm", "avi"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipInfo {
    pub width: u32,
    pub height: u32,
    pub frame_rate: f32,
    pub duration_seconds: f64,
    pub codec: String,
    pub bitrate: u64,
}

impl ClipInfo {
    pub fn badge(&self) -> String {
        let resolution = if self.height >= 2_160 {
            "4K".to_string()
        } else {
            format!("{}p", self.height)
        };
        format!("{resolution}{}", nominal_frame_rate(self.frame_rate))
    }

    pub fn summary(&self) -> String {
        format!(
            "{}×{} · {} fps · {} · {:.0} Mbps",
            self.width,
            self.height,
            nominal_frame_rate(self.frame_rate),
            self.codec,
            self.bitrate as f64 / 1_000_000.0
        )
    }
}

const COMMON_FRAME_RATES: [u32; 11] = [24, 25, 30, 48, 50, 60, 72, 90, 120, 144, 240];
const COMMON_FRAME_RATE_TOLERANCE: f32 = 0.04;

pub fn nominal_frame_rate(frame_rate: f32) -> u32 {
    COMMON_FRAME_RATES
        .iter()
        .copied()
        .find(|common| {
            (frame_rate - *common as f32).abs() <= *common as f32 * COMMON_FRAME_RATE_TOLERANCE
        })
        .unwrap_or_else(|| frame_rate.round() as u32)
}

#[derive(Clone, Debug)]
pub struct Clip {
    pub id: String,
    pub path: PathBuf,
    pub title: String,
    pub size_bytes: u64,
    pub taken: LocalDateTime,
    pub info: Option<ClipInfo>,
}

impl Clip {
    pub fn thumbnail_path(&self) -> PathBuf {
        app_paths::thumbnail_directory().join(format!("{}.jpg", self.id))
    }

    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    pub fn detail_line(&self) -> String {
        format!(
            "{} · {}",
            self.taken.clock_label(),
            format_size(self.size_bytes)
        )
    }
}

pub fn scan_folders(folders: &[FolderConfig]) -> Vec<Clip> {
    let mut seen = HashSet::new();
    let mut clips = Vec::new();
    for folder in folders.iter().filter(|folder| folder.enabled) {
        scan_directory(&folder.path, &mut seen, &mut clips);
    }
    clips
}

fn scan_directory(directory: &Path, seen: &mut HashSet<PathBuf>, clips: &mut Vec<Clip>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            scan_directory(&path, seen, clips);
        } else if metadata.len() > 0 && is_video(&path) && seen.insert(path.clone()) {
            clips.push(clip_from_file(path, &metadata));
        }
    }
}

fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            VIDEO_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
}

fn clip_from_file(path: PathBuf, metadata: &fs::Metadata) -> Clip {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parsed = clip_name::parse(&stem);
    let modified = metadata.modified().ok();
    let taken = parsed
        .taken
        .or_else(|| modified.and_then(LocalDateTime::from_system_time))
        .unwrap_or(LocalDateTime {
            date: Date {
                year: 1970,
                month: 1,
                day: 1,
            },
            seconds_of_day: 0,
        });
    let modified_seconds = modified
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs());
    let id = stable_id(&path, metadata.len(), modified_seconds);
    Clip {
        id,
        path,
        title: parsed.title,
        size_bytes: metadata.len(),
        taken,
        info: None,
    }
}

fn stable_id(path: &Path, size_bytes: u64, modified_seconds: u64) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let identity = format!(
        "{}|{size_bytes}|{modified_seconds}",
        path.to_string_lossy().to_lowercase()
    );
    for byte in identity.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOrder {
    NewestFirst,
    OldestFirst,
    Largest,
    Longest,
    Name,
}

impl SortOrder {
    pub const ALL: [Self; 5] = [
        Self::NewestFirst,
        Self::OldestFirst,
        Self::Largest,
        Self::Longest,
        Self::Name,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::NewestFirst => "Newest first",
            Self::OldestFirst => "Oldest first",
            Self::Largest => "Largest first",
            Self::Longest => "Longest first",
            Self::Name => "Name",
        }
    }

    fn groups_by_date(self) -> bool {
        matches!(self, Self::NewestFirst | Self::OldestFirst)
    }
}

pub fn visible_order(clips: &[Clip], query: &str, sort: SortOrder) -> Vec<usize> {
    let lowered = query.to_lowercase();
    let words: Vec<&str> = lowered.split_whitespace().collect();
    let mut order: Vec<usize> = (0..clips.len())
        .filter(|index| {
            let haystack =
                format!("{} {}", clips[*index].title, clips[*index].file_name()).to_lowercase();
            words.iter().all(|word| haystack.contains(word))
        })
        .collect();
    match sort {
        SortOrder::NewestFirst => {
            order.sort_by(|first, second| clips[*second].taken.cmp(&clips[*first].taken))
        }
        SortOrder::OldestFirst => {
            order.sort_by(|first, second| clips[*first].taken.cmp(&clips[*second].taken))
        }
        SortOrder::Largest => {
            order.sort_by(|first, second| clips[*second].size_bytes.cmp(&clips[*first].size_bytes))
        }
        SortOrder::Longest => order.sort_by(|first, second| {
            let duration = |index: &usize| {
                clips[*index]
                    .info
                    .as_ref()
                    .map_or(-1.0, |info| info.duration_seconds)
            };
            duration(second).total_cmp(&duration(first))
        }),
        SortOrder::Name => order.sort_by_key(|index| clips[*index].title.to_lowercase()),
    }
    order
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub label: String,
    pub caption: String,
    pub members: std::ops::Range<usize>,
}

pub fn group_order(clips: &[Clip], order: &[usize], sort: SortOrder, today: Date) -> Vec<Group> {
    if order.is_empty() {
        return Vec::new();
    }
    if !sort.groups_by_date() {
        return vec![Group {
            label: "All clips".into(),
            caption: format!("{} videos", order.len()),
            members: 0..order.len(),
        }];
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut previous_key: Option<String> = None;
    for (position, index) in order.iter().enumerate() {
        let date = clips[*index].taken.date;
        let (label, caption) = date_group_label(date, today);
        let key = format!(
            "{label}|{}",
            if caption.is_empty() {
                date.month_label()
            } else {
                String::new()
            }
        );
        if previous_key.as_deref() == Some(key.as_str()) {
            if let Some(last) = groups.last_mut() {
                last.members.end = position + 1;
            }
        } else {
            groups.push(Group {
                label,
                caption,
                members: position..position + 1,
            });
            previous_key = Some(key);
        }
    }
    groups
}

pub fn taken_label(taken: LocalDateTime, today: Date) -> String {
    let (label, _) = date_group_label(taken.date, today);
    let day = if label == "Today" || label == "Yesterday" {
        label
    } else {
        taken.date.short_label()
    };
    format!("{day}, {}", taken.clock_label())
}

fn date_group_label(date: Date, today: Date) -> (String, String) {
    match days_between(date, today) {
        0 => ("Today".into(), date.short_label()),
        1 => ("Yesterday".into(), date.short_label()),
        2..=6 => ("This week".into(), date.short_label()),
        _ => (date.month_label(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(title: &str, day: u32, size_bytes: u64) -> Clip {
        Clip {
            id: format!("{title}{size_bytes}"),
            path: PathBuf::from(format!("C:/clips/{title}.mp4")),
            title: title.to_string(),
            size_bytes,
            taken: LocalDateTime::new(
                Date {
                    year: 2026,
                    month: 11,
                    day,
                },
                12,
                0,
                0,
            )
            .unwrap(),
            info: None,
        }
    }

    const TODAY: Date = Date {
        year: 2026,
        month: 11,
        day: 21,
    };

    #[test]
    fn newest_first_groups_by_relative_day() {
        let clips = vec![
            clip("a", 21, 1),
            clip("b", 20, 1),
            clip("c", 19, 1),
            clip("d", 18, 1),
            clip("e", 2, 1),
        ];
        let order = visible_order(&clips, "", SortOrder::NewestFirst);
        let groups = group_order(&clips, &order, SortOrder::NewestFirst, TODAY);
        let labels: Vec<_> = groups.iter().map(|group| group.label.as_str()).collect();
        assert_eq!(labels, ["Today", "Yesterday", "This week", "November 2026"]);
        assert_eq!(groups[2].members, 2..4);
    }

    #[test]
    fn search_requires_every_word() {
        let clips = vec![clip("Neon Drift", 21, 1), clip("Hollow Spire", 20, 1)];
        assert_eq!(
            visible_order(&clips, "neon drift", SortOrder::Name),
            vec![0]
        );
        assert!(visible_order(&clips, "neon spire", SortOrder::Name).is_empty());
    }

    #[test]
    fn size_sort_puts_the_largest_first_and_has_one_group() {
        let clips = vec![clip("a", 21, 10), clip("b", 21, 30), clip("c", 21, 20)];
        let order = visible_order(&clips, "", SortOrder::Largest);
        assert_eq!(order, vec![1, 2, 0]);
        assert_eq!(
            group_order(&clips, &order, SortOrder::Largest, TODAY).len(),
            1
        );
    }

    #[test]
    fn badge_uses_resolution_and_nominal_frame_rate() {
        let info = |height, frame_rate| ClipInfo {
            width: 0,
            height,
            frame_rate,
            duration_seconds: 1.0,
            codec: "HEVC".into(),
            bitrate: 0,
        };
        assert_eq!(info(1440, 59.94).badge(), "1440p60");
        assert_eq!(info(2160, 60.0).badge(), "4K60");
        assert_eq!(info(1080, 143.99).badge(), "1080p144");
        assert_eq!(info(1080, 58.9).badge(), "1080p60");
        assert_eq!(nominal_frame_rate(23.976), 24);
        assert_eq!(nominal_frame_rate(75.0), 75);
    }

    #[test]
    fn stable_id_depends_on_path_size_and_time_only() {
        let path = Path::new("C:/clips/a.mp4");
        assert_eq!(stable_id(path, 1, 2), stable_id(path, 1, 2));
        assert_ne!(stable_id(path, 1, 2), stable_id(path, 2, 2));
        assert_ne!(stable_id(path, 1, 2), stable_id(path, 1, 3));
        assert_eq!(
            stable_id(path, 1, 2),
            stable_id(Path::new("C:/CLIPS/a.mp4"), 1, 2)
        );
    }
}
