use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

use anyhow::{Result, anyhow};
use cpal::SampleFormat;
use cpal::traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _};
use ffmpeg::codec;
use ffmpeg::format::{self, Sample, sample::Type as SampleType};
use ffmpeg::media::Type;
use ffmpeg::software::resampling::Context as Resampler;
use ffmpeg::{ChannelLayout, frame};
use ffmpeg_next as ffmpeg;

use crate::media::source::read_packet;

const OUTPUT_CHANNELS: usize = 2;
const BUFFER_SECONDS: f64 = 0.4;

struct Ring {
    samples: VecDeque<f32>,
    base_seconds: f64,
    consumed_samples: u64,
    generation: u64,
    seek: Option<(f64, u64)>,
    quit: bool,
}

struct Shared {
    ring: Mutex<Ring>,
    wake: Condvar,
    playing: AtomicBool,
    enabled: AtomicBool,
    volume_bits: AtomicU32,
    sample_rate: AtomicU32,
}

impl Shared {
    fn new() -> Self {
        Self {
            ring: Mutex::new(Ring {
                samples: VecDeque::new(),
                base_seconds: 0.0,
                consumed_samples: 0,
                generation: 0,
                seek: Some((0.0, 0)),
                quit: false,
            }),
            wake: Condvar::new(),
            playing: AtomicBool::new(false),
            enabled: AtomicBool::new(true),
            volume_bits: AtomicU32::new(0.8f32.to_bits()),
            sample_rate: AtomicU32::new(0),
        }
    }

    fn ring(&self) -> MutexGuard<'_, Ring> {
        self.ring
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub struct AudioPlayer {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl AudioPlayer {
    pub fn open(path: PathBuf) -> Self {
        let shared = Arc::new(Shared::new());
        let worker = thread::spawn({
            let shared = shared.clone();
            move || {
                run(&shared, &path).ok();
            }
        });
        Self {
            shared,
            worker: Some(worker),
        }
    }

    pub fn seek(&self, seconds: f64) {
        let mut ring = self.shared.ring();
        ring.generation += 1;
        ring.samples.clear();
        ring.base_seconds = seconds;
        ring.consumed_samples = 0;
        ring.seek = Some((seconds, ring.generation));
        self.shared.wake.notify_all();
    }

    pub fn set_playing(&self, playing: bool) {
        self.shared.playing.store(playing, Ordering::SeqCst);
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.shared.enabled.store(enabled, Ordering::SeqCst);
    }

    pub fn set_volume(&self, volume: f32) {
        self.shared
            .volume_bits
            .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::SeqCst);
    }

    pub fn position(&self) -> Option<f64> {
        let rate = self.shared.sample_rate.load(Ordering::SeqCst);
        let ring = self.shared.ring();
        (rate > 0 && ring.consumed_samples > 0 && !ring.samples.is_empty()).then(|| {
            ring.base_seconds
                + ring.consumed_samples as f64 / (f64::from(rate) * OUTPUT_CHANNELS as f64)
        })
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.shared.ring().quit = true;
        self.shared.wake.notify_all();
        if let Some(worker) = self.worker.take() {
            worker.join().ok();
        }
    }
}

fn run(shared: &Arc<Shared>, path: &Path) -> Result<()> {
    let mut input = format::input(path)?;
    let stream = input
        .streams()
        .best(Type::Audio)
        .ok_or_else(|| anyhow!("no audio stream"))?;
    let stream_index = stream.index();
    let time_base = stream.time_base();
    let mut decoder = codec::context::Context::from_parameters(stream.parameters())?
        .decoder()
        .audio()?;
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| anyhow!("no audio output device"))?;
    let supported = device.default_output_config()?;
    if supported.sample_format() != SampleFormat::F32 {
        return Err(anyhow!("unsupported output sample format"));
    }
    let output_rate = supported.sample_rate();
    let output_channels = usize::from(supported.channels());
    shared.sample_rate.store(output_rate, Ordering::SeqCst);
    let callback_shared = shared.clone();
    let stream_handle = device.build_output_stream(
        supported.config(),
        move |buffer: &mut [f32], _| fill_buffer(&callback_shared, buffer, output_channels),
        |error| eprintln!("audio output error: {error}"),
        None,
    )?;
    stream_handle.play()?;

    let layout = if decoder.channel_layout().is_empty() {
        ChannelLayout::default(i32::from(decoder.channels()))
    } else {
        decoder.channel_layout()
    };
    let mut resampler = Resampler::get(
        decoder.format(),
        layout,
        decoder.rate(),
        Sample::F32(SampleType::Packed),
        ChannelLayout::STEREO,
        output_rate,
    )?;
    let capacity = (f64::from(output_rate) * OUTPUT_CHANNELS as f64 * BUFFER_SECONDS) as usize;
    let mut generation = 0;
    let mut trim_until: Option<f64> = None;
    let mut ended = false;
    loop {
        {
            let mut ring = shared.ring();
            loop {
                if ring.quit {
                    return Ok(());
                }
                if let Some((seconds, new_generation)) = ring.seek.take() {
                    generation = new_generation;
                    trim_until = Some(seconds);
                    ended = false;
                    drop(ring);
                    let target = (seconds.max(0.0) * f64::from(ffmpeg::ffi::AV_TIME_BASE)) as i64;
                    input.seek(target, ..target).ok();
                    decoder.flush();
                    ring = shared.ring();
                    continue;
                }
                if !ended && ring.samples.len() < capacity {
                    break;
                }
                ring = shared
                    .wake
                    .wait_timeout(ring, std::time::Duration::from_millis(20))
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .0;
            }
        }
        let Some(decoded) = next_audio_frame(&mut input, stream_index, &mut decoder) else {
            ended = true;
            continue;
        };
        let start_seconds = decoded.pts().map(|pts| {
            pts as f64 * f64::from(time_base.numerator()) / f64::from(time_base.denominator())
        });
        let mut converted = frame::Audio::empty();
        if resampler.run(&decoded, &mut converted).is_err() || converted.samples() == 0 {
            continue;
        }
        let mut samples: Vec<f32> =
            converted.plane::<f32>(0)[..converted.samples() * OUTPUT_CHANNELS].to_vec();
        if let (Some(target), Some(start)) = (trim_until, start_seconds) {
            let frame_seconds = converted.samples() as f64 / f64::from(output_rate);
            if start + frame_seconds <= target {
                continue;
            }
            let skip =
                ((target - start).max(0.0) * f64::from(output_rate)) as usize * OUTPUT_CHANNELS;
            samples.drain(..skip.min(samples.len()));
        }
        trim_until = None;
        let mut ring = shared.ring();
        if ring.generation == generation {
            ring.samples.extend(samples);
        }
    }
}

fn next_audio_frame(
    input: &mut format::context::Input,
    stream_index: usize,
    decoder: &mut ffmpeg::decoder::Audio,
) -> Option<frame::Audio> {
    let mut decoded = frame::Audio::empty();
    loop {
        match decoder.receive_frame(&mut decoded) {
            Ok(()) => return Some(decoded),
            Err(ffmpeg::Error::Eof) => return None,
            Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => {}
            Err(_) => return None,
        }
        match read_packet(input, stream_index) {
            Ok(Some(packet)) => {
                decoder.send_packet(&packet).ok()?;
            }
            _ => return None,
        }
    }
}

fn fill_buffer(shared: &Shared, buffer: &mut [f32], output_channels: usize) {
    let audible = shared.playing.load(Ordering::SeqCst) && shared.enabled.load(Ordering::SeqCst);
    if !audible {
        buffer.fill(0.0);
        return;
    }
    let volume = f32::from_bits(shared.volume_bits.load(Ordering::SeqCst));
    let mut ring = shared.ring();
    for frame in buffer.chunks_mut(output_channels) {
        let (left, right) = match (ring.samples.pop_front(), ring.samples.pop_front()) {
            (Some(left), Some(right)) => {
                ring.consumed_samples += OUTPUT_CHANNELS as u64;
                (left, right)
            }
            _ => (0.0, 0.0),
        };
        for (channel, sample) in frame.iter_mut().enumerate() {
            *sample = if channel == 0 { left } else { right } * volume;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_is_none_once_the_audio_ran_out() {
        let shared = Shared::new();
        shared.sample_rate.store(48_000, Ordering::SeqCst);
        {
            let mut ring = shared.ring();
            ring.consumed_samples = 96_000;
            ring.samples.extend([0.0; 4]);
        }
        let audio = AudioPlayer {
            shared: Arc::new(shared),
            worker: None,
        };
        assert_eq!(audio.position(), Some(1.0));
        audio.shared.ring().samples.clear();
        assert_eq!(audio.position(), None);
    }
}
