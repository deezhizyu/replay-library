use crate::export_options::{SourceFormat, VideoCodec, scaled_width};
use crate::library::nominal_frame_rate;

const BUDGET_FRACTION: f64 = 0.95;
const MINIMUM_BITS_PER_PIXEL: f64 = 0.05;
const MINIMUM_VIDEO_BPS: f64 = 150_000.0;
const LADDER: [(u32, u32); 9] = [
    (2160, 60),
    (1440, 60),
    (1080, 60),
    (1080, 30),
    (720, 60),
    (720, 30),
    (540, 30),
    (480, 30),
    (360, 24),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressTarget {
    Megabytes8,
    Megabytes20,
    Custom(u32),
}

impl CompressTarget {
    pub fn megabytes(self) -> u32 {
        match self {
            Self::Megabytes8 => 8,
            Self::Megabytes20 => 20,
            Self::Custom(megabytes) => megabytes,
        }
    }

    pub fn target_bytes(self) -> u64 {
        u64::from(self.megabytes()) * 1_000_000
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompressPlan {
    pub width: u32,
    pub height: u32,
    pub frame_rate: u32,
    pub codec: VideoCodec,
    pub video_kbps: u32,
    pub audio_kbps: u32,
}

impl CompressPlan {
    pub fn estimated_bytes(&self, duration_seconds: f64) -> u64 {
        (f64::from(self.video_kbps + self.audio_kbps) * 1000.0 / 8.0 * duration_seconds) as u64
    }
}

fn audio_kbps_for(megabytes: u32) -> u32 {
    match megabytes {
        0..=10 => 64,
        11..=30 => 96,
        _ => 128,
    }
}

pub fn plan(target: CompressTarget, duration_seconds: f64, source: SourceFormat) -> CompressPlan {
    let duration = duration_seconds.max(0.1);
    let audio_kbps = audio_kbps_for(target.megabytes());
    let total_bps = target.target_bytes() as f64 * 8.0 * BUDGET_FRACTION / duration;
    let video_bps = (total_bps - f64::from(audio_kbps) * 1000.0).max(MINIMUM_VIDEO_BPS);
    let source_rate = nominal_frame_rate(source.frame_rate).max(1);
    let mut candidates: Vec<(u32, u32)> = LADDER
        .iter()
        .filter(|(height, _)| *height <= source.height)
        .map(|(height, rate)| (*height, (*rate).min(source_rate)))
        .collect();
    if !LADDER.iter().any(|(height, _)| *height == source.height) {
        candidates.insert(0, (source.height, source_rate.min(60)));
    }
    let chosen = candidates
        .iter()
        .find(|(height, rate)| {
            let width = f64::from(scaled_width(source, *height));
            video_bps / (width * f64::from(*height) * f64::from(*rate)) >= MINIMUM_BITS_PER_PIXEL
        })
        .or(candidates.last())
        .copied()
        .unwrap_or((source.height, source_rate));
    CompressPlan {
        width: scaled_width(source, chosen.0),
        height: chosen.0,
        frame_rate: chosen.1,
        codec: VideoCodec::H264,
        video_kbps: (video_bps / 1000.0) as u32,
        audio_kbps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: SourceFormat = SourceFormat {
        width: 2560,
        height: 1440,
        frame_rate: 60.0,
    };

    #[test]
    fn eight_megabytes_for_a_short_clip_gives_720p30() {
        let chosen = plan(CompressTarget::Megabytes8, 42.0, SOURCE);
        assert_eq!(
            (chosen.width, chosen.height, chosen.frame_rate),
            (1280, 720, 30)
        );
        assert_eq!(chosen.audio_kbps, 64);
        assert!(chosen.estimated_bytes(42.0) <= 8_000_000);
    }

    #[test]
    fn twenty_megabytes_gives_1080p30() {
        let chosen = plan(CompressTarget::Megabytes20, 42.0, SOURCE);
        assert_eq!((chosen.height, chosen.frame_rate), (1080, 30));
        assert!(chosen.estimated_bytes(42.0) <= 20_000_000);
    }

    #[test]
    fn long_clips_fall_to_the_bottom_of_the_ladder_but_stay_within_budget() {
        let chosen = plan(CompressTarget::Megabytes8, 1_800.0, SOURCE);
        assert_eq!(chosen.height, 360);
        assert!(chosen.video_kbps >= 150);
    }

    #[test]
    fn never_upscales_or_raises_the_frame_rate() {
        let small = SourceFormat {
            width: 1280,
            height: 720,
            frame_rate: 30.0,
        };
        let chosen = plan(CompressTarget::Megabytes20, 10.0, small);
        assert_eq!((chosen.height, chosen.frame_rate), (720, 30));
    }
}
