use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use ffmpeg::format::Pixel;
use ffmpeg::software::scaling::Flags;
use ffmpeg_next as ffmpeg;
use gpui::RenderImage;

use crate::library::ClipInfo;
use crate::media::convert::{FrameConverter, fit_within};
use crate::media::render_image::from_bgra;
use crate::media::source::VideoSource;

const QUEUE_CAPACITY: usize = 8;
const FALLBACK_FRAME_RATE: f64 = 60.0;

struct VideoFrame {
    presentation_seconds: f64,
    image: Arc<RenderImage>,
}

struct SeekRequest {
    seconds: f64,
    exact: bool,
    generation: u64,
}

struct State {
    frames: VecDeque<VideoFrame>,
    seek: Option<SeekRequest>,
    generation: u64,
    playing: bool,
    frames_requested: usize,
    ended: bool,
    failed: bool,
    quit: bool,
    target_size: (u32, u32),
    decoded_frames: u64,
    average_frame_milliseconds: f64,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

enum Step {
    Quit,
    Seek(SeekRequest),
    Decode,
}

pub struct PlaybackStats {
    pub queued: usize,
    pub decoded_frames: u64,
    pub average_frame_milliseconds: f64,
}

pub struct Playback {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    info: ClipInfo,
}

impl Playback {
    pub fn open(path: PathBuf, info: ClipInfo, target_size: (u32, u32)) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                frames: VecDeque::new(),
                seek: Some(SeekRequest {
                    seconds: 0.0,
                    exact: false,
                    generation: 0,
                }),
                generation: 0,
                playing: false,
                frames_requested: 1,
                ended: false,
                failed: false,
                quit: false,
                target_size,
                decoded_frames: 0,
                average_frame_milliseconds: 0.0,
            }),
            wake: Condvar::new(),
        });
        let worker = thread::spawn({
            let shared = shared.clone();
            move || decode_loop(&shared, &path)
        });
        Self {
            shared,
            worker: Some(worker),
            info,
        }
    }

    pub fn seek(&self, seconds: f64, exact: bool) {
        let mut state = self.shared.lock();
        state.generation += 1;
        state.frames.clear();
        state.ended = false;
        state.frames_requested = 1;
        state.seek = Some(SeekRequest {
            seconds,
            exact,
            generation: state.generation,
        });
        self.shared.wake.notify_all();
    }

    pub fn set_playing(&self, playing: bool) {
        let mut state = self.shared.lock();
        state.playing = playing;
        self.shared.wake.notify_all();
    }

    pub fn step_frame(&self) {
        let mut state = self.shared.lock();
        state.frames_requested += 1;
        self.shared.wake.notify_all();
    }

    pub fn set_target_size(&self, size: (u32, u32)) {
        self.shared.lock().target_size = size;
    }

    pub fn stats(&self) -> PlaybackStats {
        let state = self.shared.lock();
        PlaybackStats {
            queued: state.frames.len(),
            decoded_frames: state.decoded_frames,
            average_frame_milliseconds: state.average_frame_milliseconds,
        }
    }

    pub fn finished(&self) -> bool {
        let state = self.shared.lock();
        (state.ended && state.frames.is_empty()) || state.failed
    }

    pub fn take_frame(&self, position_seconds: f64) -> Option<Arc<RenderImage>> {
        let tolerance = 0.5 / f64::from(self.info.frame_rate.max(1.0));
        let mut state = self.shared.lock();
        let mut latest = None;
        while state
            .frames
            .front()
            .is_some_and(|frame| frame.presentation_seconds <= position_seconds + tolerance)
        {
            latest = state.frames.pop_front();
        }
        if latest.is_some() {
            self.shared.wake.notify_all();
        }
        latest.map(|frame| frame.image)
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.shared.lock().quit = true;
        self.shared.wake.notify_all();
        if let Some(worker) = self.worker.take() {
            worker.join().ok();
        }
    }
}

fn next_step(shared: &Shared, skipping: bool) -> (Step, (u32, u32)) {
    let mut state = shared.lock();
    loop {
        if state.quit {
            return (Step::Quit, state.target_size);
        }
        if let Some(request) = state.seek.take() {
            return (Step::Seek(request), state.target_size);
        }
        let wanted = if state.playing {
            QUEUE_CAPACITY.saturating_sub(state.frames.len())
        } else {
            state.frames_requested
        };
        if skipping || (wanted > 0 && !state.ended) {
            return (Step::Decode, state.target_size);
        }
        state = shared
            .wake
            .wait(state)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
    }
}

fn decode_loop(shared: &Shared, path: &Path) {
    let Ok(mut source) = VideoSource::open(path) else {
        shared.lock().failed = true;
        return;
    };
    let info = source.info();
    let frame_duration = 1.0
        / f64::from(if info.frame_rate > 1.0 {
            info.frame_rate
        } else {
            FALLBACK_FRAME_RATE as f32
        });
    let mut converter = FrameConverter::new(Pixel::BGRA, Flags::FAST_BILINEAR);
    let mut generation = 0;
    let mut skip_until: Option<f64> = None;
    let mut preview_pending = false;
    let mut last_presentation = 0.0;
    loop {
        let (step, target_size) = next_step(shared, skip_until.is_some());
        match step {
            Step::Quit => return,
            Step::Seek(request) => {
                generation = request.generation;
                skip_until = request.exact.then_some(request.seconds);
                preview_pending = request.exact;
                if source.seek_seconds(request.seconds).is_err() {
                    skip_until = None;
                }
                last_presentation = request.seconds;
            }
            Step::Decode => {}
        }
        let frame_started = Instant::now();
        match source.next_frame() {
            Ok(Some(decoded)) => {
                let presentation = source
                    .seconds_of(&decoded)
                    .unwrap_or(last_presentation + frame_duration);
                last_presentation = presentation;
                let reached =
                    skip_until.is_none_or(|target| presentation + frame_duration * 0.5 >= target);
                let show = reached || preview_pending;
                preview_pending = false;
                if reached {
                    skip_until = None;
                }
                if !show {
                    continue;
                }
                let (width, height) = fit_within(
                    decoded.width(),
                    decoded.height(),
                    target_size.0,
                    target_size.1,
                );
                let image = converter
                    .convert_packed(&decoded, width, height)
                    .and_then(|bytes| from_bgra(width, height, bytes));
                let Ok(image) = image else {
                    continue;
                };
                let mut state = shared.lock();
                state.decoded_frames += 1;
                state.average_frame_milliseconds += (frame_started.elapsed().as_secs_f64()
                    * 1000.0
                    - state.average_frame_milliseconds)
                    * 0.1;
                if state.generation == generation {
                    state.frames.push_back(VideoFrame {
                        presentation_seconds: presentation,
                        image,
                    });
                    if !state.playing {
                        state.frames_requested = state.frames_requested.saturating_sub(1);
                    }
                }
            }
            Ok(None) | Err(_) => {
                skip_until = None;
                let mut state = shared.lock();
                if state.generation == generation {
                    state.ended = true;
                }
            }
        }
    }
}
