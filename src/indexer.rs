use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN,
};

use crate::library::ClipInfo;
use crate::media::thumbnail::probe_and_write_thumbnail;

#[derive(Clone)]
pub struct IndexJob {
    pub cache_id: String,
    pub clip_path: PathBuf,
    pub thumbnail_path: PathBuf,
}

pub struct IndexResult {
    pub cache_id: String,
    pub info: Option<ClipInfo>,
}

#[derive(Default)]
struct Queues {
    urgent: Vec<IndexJob>,
    background: VecDeque<IndexJob>,
    known: HashSet<String>,
    finished: HashSet<String>,
}

struct Shared {
    queues: Mutex<Queues>,
    wake: Condvar,
}

pub struct Indexer {
    shared: Arc<Shared>,
    results: Receiver<IndexResult>,
}

impl Indexer {
    pub fn start() -> Self {
        let shared = Arc::new(Shared {
            queues: Mutex::new(Queues::default()),
            wake: Condvar::new(),
        });
        let (sender, results) = channel();
        let worker_count =
            thread::available_parallelism().map_or(2, |count| (count.get() / 4).max(2));
        for _ in 0..worker_count {
            let shared = shared.clone();
            let sender = sender.clone();
            thread::spawn(move || work(&shared, &sender));
        }
        Self { shared, results }
    }

    pub fn enqueue_background(&self, jobs: Vec<IndexJob>) {
        let mut queues = self
            .shared
            .queues
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for job in jobs {
            if queues.known.insert(job.cache_id.clone()) {
                queues.background.push_back(job);
            }
        }
        self.shared.wake.notify_all();
    }

    pub fn prioritize(&self, job: IndexJob) {
        let mut queues = self
            .shared
            .queues
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if queues.finished.contains(&job.cache_id)
            || queues
                .urgent
                .iter()
                .any(|queued| queued.cache_id == job.cache_id)
        {
            return;
        }
        queues.known.insert(job.cache_id.clone());
        queues.urgent.push(job);
        self.shared.wake.notify_one();
    }

    pub fn drain_results(&self) -> Vec<IndexResult> {
        self.results.try_iter().collect()
    }

    pub fn remaining(&self) -> usize {
        let queues = self
            .shared
            .queues
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        queues.urgent.len() + queues.background.len()
    }
}

fn work(shared: &Shared, sender: &Sender<IndexResult>) {
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN) }.ok();
    loop {
        let job = {
            let mut queues = shared
                .queues
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            loop {
                if let Some(job) = queues
                    .urgent
                    .pop()
                    .or_else(|| queues.background.pop_front())
                {
                    break job;
                }
                queues = shared
                    .wake
                    .wait(queues)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        };
        let info = probe_and_write_thumbnail(&job.clip_path, &job.thumbnail_path).ok();
        shared
            .queues
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .finished
            .insert(job.cache_id.clone());
        if sender
            .send(IndexResult {
                cache_id: job.cache_id,
                info,
            })
            .is_err()
        {
            return;
        }
    }
}
