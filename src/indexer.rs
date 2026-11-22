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
use crate::mutex_ext::MutexExt as _;

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
    claimed: HashSet<String>,
}

impl Queues {
    fn enqueue(&mut self, job: IndexJob) {
        if self.known.insert(job.cache_id.clone()) {
            self.background.push_back(job);
        }
    }

    fn prioritize(&mut self, job: IndexJob) {
        let queued = self
            .urgent
            .iter()
            .any(|queued| queued.cache_id == job.cache_id);
        if queued || self.claimed.contains(&job.cache_id) {
            return;
        }
        self.known.insert(job.cache_id.clone());
        self.urgent.push(job);
    }

    fn claim_next(&mut self) -> Option<IndexJob> {
        while let Some(job) = self.urgent.pop().or_else(|| self.background.pop_front()) {
            if self.claimed.insert(job.cache_id.clone()) {
                return Some(job);
            }
        }
        None
    }

    fn forget_all(&mut self) {
        self.known.clear();
        self.claimed.clear();
    }
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
        let mut queues = self.shared.queues.lock_ignoring_poison();
        for job in jobs {
            queues.enqueue(job);
        }
        self.shared.wake.notify_all();
    }

    pub fn prioritize(&self, job: IndexJob) {
        self.shared.queues.lock_ignoring_poison().prioritize(job);
        self.shared.wake.notify_one();
    }

    pub fn forget_all(&self) {
        self.shared.queues.lock_ignoring_poison().forget_all();
    }

    pub fn drain_results(&self) -> Vec<IndexResult> {
        self.results.try_iter().collect()
    }

    pub fn remaining(&self) -> usize {
        let queues = self.shared.queues.lock_ignoring_poison();
        queues.urgent.len() + queues.background.len()
    }
}

fn work(shared: &Shared, sender: &Sender<IndexResult>) {
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN) }.ok();
    loop {
        let job = {
            let mut queues = shared.queues.lock_ignoring_poison();
            loop {
                if let Some(job) = queues.claim_next() {
                    break job;
                }
                queues = shared
                    .wake
                    .wait(queues)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        };
        let info = probe_and_write_thumbnail(&job.clip_path, &job.thumbnail_path).ok();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str) -> IndexJob {
        IndexJob {
            cache_id: id.into(),
            clip_path: PathBuf::from(format!("{id}.mp4")),
            thumbnail_path: PathBuf::from(format!("{id}.jpg")),
        }
    }

    #[test]
    fn a_prioritized_clip_is_not_indexed_again_from_the_background_queue() {
        let mut queues = Queues::default();
        queues.enqueue(job("a"));
        queues.prioritize(job("a"));
        assert_eq!(queues.claim_next().unwrap().cache_id, "a");
        assert!(queues.claim_next().is_none());
    }

    #[test]
    fn a_clip_being_indexed_is_not_queued_again() {
        let mut queues = Queues::default();
        queues.prioritize(job("a"));
        assert!(queues.claim_next().is_some());
        queues.prioritize(job("a"));
        assert!(queues.claim_next().is_none());
    }

    #[test]
    fn forgetting_everything_lets_a_clip_be_indexed_again() {
        let mut queues = Queues::default();
        queues.enqueue(job("a"));
        assert!(queues.claim_next().is_some());
        queues.forget_all();
        queues.enqueue(job("a"));
        assert!(queues.claim_next().is_some());
    }
}
