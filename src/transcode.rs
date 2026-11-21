use std::ffi::OsString;
use std::io::{BufRead as _, BufReader, Read as _};
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow};

use crate::export_options::{AudioChoice, VideoCodec};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const OVERSHOOT_SAFETY: f64 = 0.96;
const MAXIMUM_RATE_HEADROOM: f64 = 1.2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Encoder {
    pub name: &'static str,
    pub hardware: bool,
}

const CANDIDATES: [(VideoCodec, [Encoder; 4]); 3] = [
    (
        VideoCodec::Hevc,
        [
            Encoder {
                name: "hevc_nvenc",
                hardware: true,
            },
            Encoder {
                name: "hevc_amf",
                hardware: true,
            },
            Encoder {
                name: "hevc_qsv",
                hardware: true,
            },
            Encoder {
                name: "libkvazaar",
                hardware: false,
            },
        ],
    ),
    (
        VideoCodec::Av1,
        [
            Encoder {
                name: "av1_nvenc",
                hardware: true,
            },
            Encoder {
                name: "av1_amf",
                hardware: true,
            },
            Encoder {
                name: "av1_qsv",
                hardware: true,
            },
            Encoder {
                name: "libsvtav1",
                hardware: false,
            },
        ],
    ),
    (
        VideoCodec::H264,
        [
            Encoder {
                name: "h264_nvenc",
                hardware: true,
            },
            Encoder {
                name: "h264_amf",
                hardware: true,
            },
            Encoder {
                name: "h264_qsv",
                hardware: true,
            },
            Encoder {
                name: "libopenh264",
                hardware: false,
            },
        ],
    ),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Encoders {
    hevc: Option<Encoder>,
    av1: Option<Encoder>,
    h264: Option<Encoder>,
}

impl Encoders {
    pub fn for_codec(&self, codec: VideoCodec) -> Option<Encoder> {
        match codec {
            VideoCodec::Hevc => self.hevc,
            VideoCodec::Av1 => self.av1,
            VideoCodec::H264 => self.h264,
        }
    }
}

pub struct EncoderCatalog(Arc<Mutex<Option<Encoders>>>);

impl gpui::Global for EncoderCatalog {}

impl EncoderCatalog {
    pub fn start() -> Self {
        let slot = Arc::new(Mutex::new(None));
        thread::spawn({
            let slot = slot.clone();
            move || {
                let detected = detect_encoders();
                *slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(detected);
            }
        });
        Self(slot)
    }

    pub fn encoders(&self) -> Option<Encoders> {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub fn ffmpeg_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|executable| {
            executable
                .parent()
                .map(|directory| directory.join("ffmpeg.exe"))
        })
        .unwrap_or_else(|| PathBuf::from("ffmpeg.exe"))
}

fn ffmpeg_command() -> Command {
    let mut command = Command::new(ffmpeg_path());
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

pub fn detect_encoders() -> Encoders {
    let handles: Vec<_> = CANDIDATES
        .iter()
        .map(|(codec, candidates)| {
            let codec = *codec;
            let candidates = *candidates;
            thread::spawn(move || {
                (
                    codec,
                    candidates
                        .into_iter()
                        .find(|encoder| encoder_works(encoder.name)),
                )
            })
        })
        .collect();
    let mut encoders = Encoders::default();
    for (codec, found) in handles.into_iter().filter_map(|handle| handle.join().ok()) {
        match codec {
            VideoCodec::Hevc => encoders.hevc = found,
            VideoCodec::Av1 => encoders.av1 = found,
            VideoCodec::H264 => encoders.h264 = found,
        }
    }
    encoders
}

fn encoder_works(name: &str) -> bool {
    ffmpeg_command()
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=640x360:r=30:d=0.2",
            "-frames:v",
            "3",
            "-c:v",
            name,
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[derive(Clone, Debug)]
pub struct JobSpec {
    pub input: PathBuf,
    pub output: PathBuf,
    pub start_seconds: f64,
    pub duration_seconds: f64,
    pub width: u32,
    pub height: u32,
    pub frame_rate: u32,
    pub source_height: u32,
    pub source_frame_rate: f32,
    pub encoder: Encoder,
    pub codec: VideoCodec,
    pub audio: AudioChoice,
    pub video_kbps: u32,
    pub audio_kbps: u32,
    pub target_bytes: Option<u64>,
}

fn rate_arguments(encoder: &str, video_kbps: u32) -> Vec<String> {
    let maximum = (f64::from(video_kbps) * MAXIMUM_RATE_HEADROOM) as u32;
    let buffer = video_kbps * 2;
    let bitrate = format!("{video_kbps}k");
    let maximum = format!("{maximum}k");
    let buffer = format!("{buffer}k");
    let words: &[&str] = match encoder {
        name if name.ends_with("_amf") => &[
            "-rc", "vbr_peak", "-quality", "balanced", "-b:v", &bitrate, "-maxrate", &maximum,
        ],
        name if name.ends_with("_nvenc") => &[
            "-preset", "p5", "-rc", "vbr", "-b:v", &bitrate, "-maxrate", &maximum, "-bufsize",
            &buffer,
        ],
        name if name.ends_with("_qsv") => {
            &["-preset", "medium", "-b:v", &bitrate, "-maxrate", &maximum]
        }
        "libsvtav1" => &["-preset", "8", "-b:v", &bitrate],
        "libopenh264" => &["-rc_mode", "bitrate", "-b:v", &bitrate],
        _ => &["-b:v", &bitrate],
    };
    words.iter().map(|word| (*word).to_string()).collect()
}

fn audio_arguments(spec: &JobSpec) -> Vec<String> {
    let words: Vec<String> = match spec.audio {
        AudioChoice::Aac => vec![
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            format!("{}k", spec.audio_kbps),
        ],
        AudioChoice::Opus => vec![
            "-c:a".into(),
            "libopus".into(),
            "-b:a".into(),
            format!("{}k", spec.audio_kbps),
        ],
        AudioChoice::CopySource => vec!["-c:a".into(), "copy".into()],
    };
    words
}

fn video_filters(spec: &JobSpec) -> Option<String> {
    let mut filters = Vec::new();
    if spec.height != spec.source_height {
        filters.push(format!(
            "scale={}:{}:flags=bicubic",
            spec.width, spec.height
        ));
    }
    if spec.frame_rate != spec.source_frame_rate.round() as u32 {
        filters.push(format!("fps={}", spec.frame_rate));
    }
    (!filters.is_empty()).then(|| filters.join(","))
}

pub fn build_arguments(spec: &JobSpec, partial_output: &Path) -> Vec<OsString> {
    let mut arguments: Vec<OsString> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-nostdin",
        "-progress",
        "pipe:1",
        "-nostats",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    let mut push = |words: Vec<String>| arguments.extend(words.into_iter().map(OsString::from));
    push(vec!["-ss".into(), format!("{:.3}", spec.start_seconds)]);
    arguments.push("-i".into());
    arguments.push(spec.input.as_os_str().to_owned());
    let mut push = |words: Vec<String>| arguments.extend(words.into_iter().map(OsString::from));
    push(vec![
        "-t".into(),
        format!("{:.3}", spec.duration_seconds),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "0:a:0?".into(),
    ]);
    if let Some(filters) = video_filters(spec) {
        push(vec!["-vf".into(), filters]);
    }
    push(vec!["-c:v".into(), spec.encoder.name.into()]);
    push(rate_arguments(spec.encoder.name, spec.video_kbps));
    if spec.codec == VideoCodec::Hevc {
        push(vec!["-tag:v".into(), "hvc1".into()]);
    }
    push(audio_arguments(spec));
    push(vec![
        "-movflags".into(),
        "+faststart".into(),
        "-f".into(),
        "mp4".into(),
    ]);
    arguments.push(partial_output.as_os_str().to_owned());
    arguments
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProgressSnapshot {
    pub output_seconds: f64,
    pub speed: f64,
}

pub fn apply_progress_line(line: &str, snapshot: &mut ProgressSnapshot) -> bool {
    let Some((key, value)) = line.trim().split_once('=') else {
        return false;
    };
    match key {
        "out_time_us" | "out_time_ms" => {
            if let Ok(microseconds) = value.trim().parse::<i64>() {
                snapshot.output_seconds = (microseconds as f64 / 1_000_000.0).max(0.0);
            }
        }
        "speed" => {
            snapshot.speed = value
                .trim()
                .trim_end_matches('x')
                .trim()
                .parse()
                .unwrap_or(snapshot.speed)
        }
        "progress" => return true,
        _ => {}
    }
    false
}

pub fn next_pass_bitrate(video_kbps: u32, target_bytes: u64, actual_bytes: u64) -> Option<u32> {
    (actual_bytes > target_bytes).then(|| {
        (f64::from(video_kbps) * target_bytes as f64 / actual_bytes as f64 * OVERSHOOT_SAFETY)
            as u32
    })
}

#[derive(Clone, Debug, PartialEq)]
pub enum JobPhase {
    Running {
        fraction: f64,
        speed: f64,
        elapsed_seconds: f64,
        remaining_seconds: f64,
        pass: u32,
    },
    Done {
        bytes: u64,
        took_seconds: f64,
    },
    Failed(String),
    Cancelled,
}

pub struct JobHandle {
    phase: Arc<Mutex<JobPhase>>,
    cancelled: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

impl JobHandle {
    pub fn phase(&self) -> JobPhase {
        self.phase
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(child) = self
            .child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_mut()
        {
            child.kill().ok();
        }
    }
}

pub fn start(spec: JobSpec) -> JobHandle {
    let handle = JobHandle {
        phase: Arc::new(Mutex::new(JobPhase::Running {
            fraction: 0.0,
            speed: 0.0,
            elapsed_seconds: 0.0,
            remaining_seconds: 0.0,
            pass: 1,
        })),
        cancelled: Arc::new(AtomicBool::new(false)),
        child: Arc::new(Mutex::new(None)),
    };
    let phase = handle.phase.clone();
    let cancelled = handle.cancelled.clone();
    let child = handle.child.clone();
    thread::spawn(move || {
        let outcome = run_passes(spec, &phase, &cancelled, &child);
        let mut phase = phase
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *phase = match outcome {
            _ if cancelled.load(Ordering::SeqCst) => JobPhase::Cancelled,
            Ok(done) => done,
            Err(error) => JobPhase::Failed(format!("{error:#}")),
        };
    });
    handle
}

fn run_passes(
    mut spec: JobSpec,
    phase: &Mutex<JobPhase>,
    cancelled: &AtomicBool,
    child_slot: &Mutex<Option<Child>>,
) -> Result<JobPhase> {
    let started = Instant::now();
    let partial = spec.output.with_extension("mp4.part");
    let mut pass = 1;
    loop {
        run_ffmpeg(&spec, &partial, phase, cancelled, child_slot, started, pass)?;
        let bytes = std::fs::metadata(&partial)
            .context("the encoder produced no output file")?
            .len();
        let retry = spec
            .target_bytes
            .and_then(|target| next_pass_bitrate(spec.video_kbps, target, bytes))
            .filter(|_| pass == 1);
        match retry {
            Some(kbps) if !cancelled.load(Ordering::SeqCst) => {
                spec.video_kbps = kbps;
                pass += 1;
            }
            _ => {
                std::fs::rename(&partial, &spec.output)
                    .context("failed to move the finished file into place")?;
                return Ok(JobPhase::Done {
                    bytes,
                    took_seconds: started.elapsed().as_secs_f64(),
                });
            }
        }
    }
}

fn run_ffmpeg(
    spec: &JobSpec,
    partial: &Path,
    phase: &Mutex<JobPhase>,
    cancelled: &AtomicBool,
    child_slot: &Mutex<Option<Child>>,
    started: Instant,
    pass: u32,
) -> Result<()> {
    let mut command = ffmpeg_command();
    command
        .args(build_arguments(spec, partial))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().context("failed to start ffmpeg")?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("ffmpeg has no output pipe"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("ffmpeg has no error pipe"))?;
    *child_slot
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(child);
    let error_reader = thread::spawn(move || {
        let mut text = String::new();
        stderr.read_to_string(&mut text).ok();
        text
    });
    let mut snapshot = ProgressSnapshot::default();
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if apply_progress_line(&line, &mut snapshot) {
            let fraction = (snapshot.output_seconds / spec.duration_seconds).clamp(0.0, 1.0);
            let remaining = if snapshot.speed > 0.0 {
                (spec.duration_seconds - snapshot.output_seconds).max(0.0) / snapshot.speed
            } else {
                0.0
            };
            *phase
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = JobPhase::Running {
                fraction,
                speed: snapshot.speed,
                elapsed_seconds: started.elapsed().as_secs_f64(),
                remaining_seconds: remaining,
                pass,
            };
        }
    }
    let status = loop {
        let mut slot = child_slot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(child) = slot.as_mut() else {
            return Err(anyhow!("ffmpeg process vanished"));
        };
        if let Some(status) = child.try_wait()? {
            *slot = None;
            break status;
        }
        drop(slot);
        thread::sleep(Duration::from_millis(20));
    };
    let messages = error_reader.join().unwrap_or_default();
    if cancelled.load(Ordering::SeqCst) {
        std::fs::remove_file(partial).ok();
        return Ok(());
    }
    if !status.success() {
        std::fs::remove_file(partial).ok();
        return Err(anyhow!(
            "ffmpeg failed: {}",
            messages.lines().last().unwrap_or("unknown error")
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> JobSpec {
        JobSpec {
            input: PathBuf::from("C:/clips/in.mp4"),
            output: PathBuf::from("D:/out/clip.mp4"),
            start_seconds: 12.3,
            duration_seconds: 19.5,
            width: 1920,
            height: 1080,
            frame_rate: 30,
            source_height: 1440,
            source_frame_rate: 59.94,
            encoder: Encoder {
                name: "hevc_amf",
                hardware: true,
            },
            codec: VideoCodec::Hevc,
            audio: AudioChoice::Aac,
            video_kbps: 12_000,
            audio_kbps: 160,
            target_bytes: None,
        }
    }

    fn joined(spec: &JobSpec) -> String {
        build_arguments(spec, Path::new("D:/out/clip.mp4.part"))
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn arguments_seek_before_the_input_and_set_filters_and_encoder() {
        let text = joined(&spec());
        assert!(text.contains("-ss 12.300 -i C:/clips/in.mp4 -t 19.500"));
        assert!(text.contains("-vf scale=1920:1080:flags=bicubic,fps=30"));
        assert!(text.contains("-c:v hevc_amf -rc vbr_peak"));
        assert!(text.contains("-b:v 12000k -maxrate 14400k"));
        assert!(text.contains("-tag:v hvc1"));
        assert!(text.contains("-c:a aac -b:a 160k"));
        assert!(text.ends_with("-movflags +faststart -f mp4 D:/out/clip.mp4.part"));
    }

    #[test]
    fn unchanged_resolution_and_rate_add_no_filter_and_audio_can_be_copied() {
        let mut unchanged = spec();
        unchanged.height = 1440;
        unchanged.frame_rate = 60;
        unchanged.audio = AudioChoice::CopySource;
        unchanged.codec = VideoCodec::H264;
        unchanged.encoder = Encoder {
            name: "libopenh264",
            hardware: false,
        };
        let text = joined(&unchanged);
        assert!(!text.contains("-vf"));
        assert!(!text.contains("hvc1"));
        assert!(text.contains("-c:a copy"));
        assert!(text.contains("-rc_mode bitrate"));
    }

    #[test]
    fn progress_blocks_are_parsed() {
        let mut snapshot = ProgressSnapshot::default();
        assert!(!apply_progress_line("out_time_us=5500000", &mut snapshot));
        assert!(!apply_progress_line("speed=11.4x", &mut snapshot));
        assert!(apply_progress_line("progress=continue", &mut snapshot));
        assert_eq!(
            snapshot,
            ProgressSnapshot {
                output_seconds: 5.5,
                speed: 11.4
            }
        );
        assert!(!apply_progress_line("speed=N/A", &mut snapshot));
        assert_eq!(snapshot.speed, 11.4);
    }

    #[test]
    fn overshoot_triggers_a_corrected_second_pass() {
        assert_eq!(next_pass_bitrate(1_000, 8_000_000, 7_900_000), None);
        assert_eq!(next_pass_bitrate(1_000, 8_000_000, 10_000_000), Some(768));
    }
}
