use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};

use gpui::{Context, Task};

use crate::app_paths;
use crate::indexer::{IndexJob, Indexer};
use crate::library::{self, Clip, Group, SortOrder, group_order, visible_order};
use crate::local_time::{Date, LocalDateTime};
use crate::media::filmstrip::{Filmstrip, HOVER_TILE_COUNT};
use crate::probe_cache::{self, ProbeEntries};
use crate::settings::FolderConfig;

const POLL_INTERVAL: Duration = Duration::from_millis(80);
const PROBE_SAVE_INTERVAL: Duration = Duration::from_secs(3);
const MAXIMUM_HOVER_STRIPS: usize = 24;
const RESCAN_QUIET_PERIOD: Duration = Duration::from_millis(2_500);

struct ScanOutput {
    clips: Vec<Clip>,
    probes: ProbeEntries,
    thumbnails_on_disk: HashSet<String>,
}

pub struct LibraryModel {
    pub clips: Vec<Clip>,
    pub order: Vec<usize>,
    pub groups: Vec<Group>,
    pub query: String,
    pub sort: SortOrder,
    pub scanning: bool,
    pub revision: u64,
    index_by_id: HashMap<String, usize>,
    probes: ProbeEntries,
    probes_dirty: bool,
    thumbnails_ready: HashSet<String>,
    hover_strips: HashMap<String, Filmstrip>,
    indexer: Indexer,
    today: Date,
    folders: Vec<FolderConfig>,
    watch_enabled: bool,
    watcher: Option<RecommendedWatcher>,
    watch_events: Option<Receiver<()>>,
    rescan_due: Option<Instant>,
    scan_task: Option<Task<()>>,
    _poll_task: Task<()>,
    _save_task: Task<()>,
}

impl LibraryModel {
    pub fn new(folders: Vec<FolderConfig>, cx: &mut Context<Self>) -> Self {
        let poll_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                if this
                    .update(cx, |model, cx| model.apply_indexed(cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let save_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PROBE_SAVE_INTERVAL).await;
                let Ok(snapshot) = this.update(cx, |model, _| model.take_dirty_probes()) else {
                    break;
                };
                if let Some(entries) = snapshot {
                    cx.background_executor()
                        .spawn(async move { probe_cache::save(&entries).ok() })
                        .await;
                }
            }
        });
        let mut model = Self {
            clips: Vec::new(),
            order: Vec::new(),
            groups: Vec::new(),
            query: String::new(),
            sort: SortOrder::NewestFirst,
            scanning: true,
            revision: 0,
            index_by_id: HashMap::new(),
            probes: ProbeEntries::new(),
            probes_dirty: false,
            thumbnails_ready: HashSet::new(),
            hover_strips: HashMap::new(),
            indexer: Indexer::start(),
            today: LocalDateTime::now().map_or(
                Date {
                    year: 1970,
                    month: 1,
                    day: 1,
                },
                |now| now.date,
            ),
            folders: Vec::new(),
            watch_enabled: true,
            watcher: None,
            watch_events: None,
            rescan_due: None,
            scan_task: None,
            _poll_task: poll_task,
            _save_task: save_task,
        };
        model.set_folders(folders, true, cx);
        model
    }

    pub fn set_folders(
        &mut self,
        folders: Vec<FolderConfig>,
        watch_enabled: bool,
        cx: &mut Context<Self>,
    ) {
        self.folders = folders;
        self.watch_enabled = watch_enabled;
        self.restart_watcher();
        self.rescan(cx);
    }

    fn restart_watcher(&mut self) {
        self.watcher = None;
        self.watch_events = None;
        if !self.watch_enabled {
            return;
        }
        let (sender, receiver) = channel();
        let Ok(mut watcher) =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if event.is_ok() {
                    sender.send(()).ok();
                }
            })
        else {
            return;
        };
        for folder in self.folders.iter().filter(|folder| folder.enabled) {
            watcher.watch(&folder.path, RecursiveMode::Recursive).ok();
        }
        self.watcher = Some(watcher);
        self.watch_events = Some(receiver);
    }

    pub fn folder_summary(&self, folder: &FolderConfig) -> (usize, u64) {
        self.clips
            .iter()
            .filter(|clip| clip.path.starts_with(&folder.path))
            .fold((0, 0), |(count, bytes), clip| {
                (count + 1, bytes + clip.size_bytes)
            })
    }

    pub fn rebuild_thumbnails(&mut self, cx: &mut Context<Self>) {
        self.clear_thumbnail_files();
        self.probes.clear();
        self.probes_dirty = true;
        self.rescan(cx);
    }

    pub fn clear_cache(&mut self, cx: &mut Context<Self>) {
        self.clear_thumbnail_files();
        self.probes.clear();
        self.probes_dirty = true;
        cx.notify();
    }

    fn clear_thumbnail_files(&mut self) {
        fs::remove_dir_all(app_paths::thumbnail_directory()).ok();
        self.thumbnails_ready.clear();
    }

    pub fn rescan(&mut self, cx: &mut Context<Self>) {
        self.rescan_due = None;
        let folders = self.folders.clone();
        self.scanning = true;
        self.scan_task = Some(cx.spawn(async move |this, cx| {
            let output = cx
                .background_executor()
                .spawn(async move { scan(&folders) })
                .await;
            this.update(cx, |model, cx| model.finish_scan(output, cx))
                .ok();
        }));
        cx.notify();
    }

    fn finish_scan(&mut self, output: ScanOutput, cx: &mut Context<Self>) {
        self.clips = output.clips;
        self.probes = output.probes;
        self.thumbnails_ready = output.thumbnails_on_disk;
        for clip in &mut self.clips {
            clip.info = self.probes.get(&clip.id).cloned();
        }
        self.index_by_id = self
            .clips
            .iter()
            .enumerate()
            .map(|(index, clip)| (clip.id.clone(), index))
            .collect();
        self.scanning = false;
        self.refresh_view();
        self.enqueue_missing();
        cx.notify();
    }

    fn enqueue_missing(&self) {
        let mut missing: Vec<&Clip> = self
            .clips
            .iter()
            .filter(|clip| clip.info.is_none() || !self.thumbnails_ready.contains(&clip.id))
            .collect();
        missing.sort_by_key(|clip| std::cmp::Reverse(clip.taken));
        self.indexer
            .enqueue_background(missing.into_iter().map(index_job).collect());
    }

    fn refresh_view(&mut self) {
        self.order = visible_order(&self.clips, &self.query, self.sort);
        self.groups = group_order(&self.clips, &self.order, self.sort, self.today);
        self.revision += 1;
    }

    pub fn set_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.query = query;
        self.refresh_view();
        cx.notify();
    }

    pub fn set_sort(&mut self, sort: SortOrder, cx: &mut Context<Self>) {
        self.sort = sort;
        self.refresh_view();
        cx.notify();
    }

    pub fn hover_strip(&mut self, clip_index: usize) -> Option<Filmstrip> {
        let clip = self.clips.get(clip_index)?;
        let duration = clip.info.as_ref()?.duration_seconds;
        if !self.hover_strips.contains_key(&clip.id)
            && self.hover_strips.len() >= MAXIMUM_HOVER_STRIPS
        {
            self.hover_strips.clear();
        }
        let strip = self
            .hover_strips
            .entry(clip.id.clone())
            .or_insert_with(|| Filmstrip::start(clip.path.clone(), duration, HOVER_TILE_COUNT));
        Some(strip.clone())
    }

    pub fn thumbnail_ready(&self, clip: &Clip) -> bool {
        self.thumbnails_ready.contains(&clip.id)
    }

    pub fn request_thumbnail(&self, clip: &Clip) {
        if clip.info.is_none() || !self.thumbnails_ready.contains(&clip.id) {
            self.indexer.prioritize(index_job(clip));
        }
    }

    pub fn indexing_remaining(&self) -> usize {
        self.indexer.remaining()
    }

    pub fn total_size_bytes(&self) -> u64 {
        self.clips.iter().map(|clip| clip.size_bytes).sum()
    }

    fn apply_indexed(&mut self, cx: &mut Context<Self>) {
        self.poll_folder_changes(cx);
        let results = self.indexer.drain_results();
        if results.is_empty() {
            return;
        }
        let mut order_depends_on_info = false;
        for result in results {
            let Some(&index) = self.index_by_id.get(&result.cache_id) else {
                continue;
            };
            if let Some(info) = result.info {
                self.thumbnails_ready.insert(result.cache_id.clone());
                self.probes.insert(result.cache_id, info.clone());
                self.probes_dirty = true;
                self.clips[index].info = Some(info);
                order_depends_on_info = true;
            }
        }
        if order_depends_on_info && self.sort == SortOrder::Longest {
            self.refresh_view();
        }
        cx.notify();
    }

    fn poll_folder_changes(&mut self, cx: &mut Context<Self>) {
        let changed = self
            .watch_events
            .as_ref()
            .is_some_and(|events| events.try_iter().count() > 0);
        if changed {
            self.rescan_due = Some(Instant::now() + RESCAN_QUIET_PERIOD);
        }
        if self.rescan_due.is_some_and(|due| Instant::now() >= due) {
            self.rescan(cx);
        }
    }

    fn take_dirty_probes(&mut self) -> Option<ProbeEntries> {
        self.probes_dirty.then(|| {
            self.probes_dirty = false;
            self.probes.clone()
        })
    }
}

fn index_job(clip: &Clip) -> IndexJob {
    IndexJob {
        cache_id: clip.id.clone(),
        clip_path: clip.path.clone(),
        thumbnail_path: clip.thumbnail_path(),
    }
}

fn scan(folders: &[FolderConfig]) -> ScanOutput {
    let clips = library::scan_folders(folders);
    let thumbnails_on_disk = fs::read_dir(app_paths::thumbnail_directory())
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    entry
                        .path()
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                })
                .collect()
        })
        .unwrap_or_default();
    ScanOutput {
        clips,
        probes: probe_cache::load(),
        thumbnails_on_disk,
    }
}
