use crate::library::{ClipInfo, nominal_frame_rate};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoCodec {
    Hevc,
    Av1,
    H264,
}

impl VideoCodec {
    pub const ALL: [Self; 3] = [Self::Hevc, Self::Av1, Self::H264];

    pub fn label(self) -> &'static str {
        match self {
            Self::Hevc => "HEVC (H.265)",
            Self::Av1 => "AV1",
            Self::H264 => "H.264",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Hevc => "HEVC",
            Self::Av1 => "AV1",
            Self::H264 => "H.264",
        }
    }

    pub fn tagline(self) -> &'static str {
        match self {
            Self::Hevc => "Small files, great quality",
            Self::Av1 => "Smallest files",
            Self::H264 => "Plays everywhere",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|codec| codec.short_label() == label)
    }

    pub fn bits_per_pixel(self) -> f64 {
        match self {
            Self::Hevc => 0.09,
            Self::Av1 => 0.07,
            Self::H264 => 0.14,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioChoice {
    Aac,
    Opus,
    CopySource,
}

impl AudioChoice {
    pub const ALL: [Self; 3] = [Self::Aac, Self::Opus, Self::CopySource];

    pub fn label(self) -> &'static str {
        match self {
            Self::Aac => "AAC",
            Self::Opus => "Opus",
            Self::CopySource => "Copy source",
        }
    }

    fn from_label(label: &str) -> Option<Self> {
        [Self::Aac, Self::Opus]
            .into_iter()
            .find(|choice| choice.label() == label)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceFormat {
    pub width: u32,
    pub height: u32,
    pub frame_rate: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceStreams {
    pub video_codec: VideoCodec,
    pub video_kbps: u32,
    pub audio: AudioChoice,
    pub audio_kbps: u32,
}

impl SourceStreams {
    pub fn from_info(info: &ClipInfo, source: SourceFormat) -> Self {
        let audio_kbps = if info.audio_kbps == 0 {
            FALLBACK_AUDIO_KBPS
        } else {
            info.audio_kbps
        };
        let video_codec = VideoCodec::from_label(&info.codec).unwrap_or(VideoCodec::H264);
        let video_kbps = (info.bitrate / 1000)
            .checked_sub(u64::from(info.audio_kbps))
            .filter(|kbps| *kbps > 0)
            .map_or_else(
                || {
                    pixel_based_kbps(
                        source,
                        source.height,
                        nominal_frame_rate(source.frame_rate),
                        video_codec,
                    )
                },
                |kbps| u32::try_from(kbps).unwrap_or(MAXIMUM_VIDEO_KBPS),
            );
        Self {
            video_codec,
            video_kbps,
            audio: AudioChoice::from_label(&info.audio_codec).unwrap_or(AudioChoice::Aac),
            audio_kbps,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExportOptions {
    pub height: u32,
    pub frame_rate: u32,
    pub codec: VideoCodec,
    pub audio: AudioChoice,
    pub video_kbps: u32,
    pub audio_kbps: u32,
    pub video_kbps_edited: bool,
    pub streams: SourceStreams,
}

const RESOLUTION_LADDER: [u32; 4] = [1440, 1080, 720, 480];
const FRAME_RATE_LADDER: [u32; 2] = [30, 24];
const FALLBACK_AUDIO_KBPS: u32 = 128;
const AUDIO_BITRATE_STEPS: [u32; 9] = [32, 48, 64, 96, 128, 160, 192, 256, 320];
pub const MINIMUM_VIDEO_KBPS: u32 = 300;
pub const MAXIMUM_VIDEO_KBPS: u32 = 120_000;
pub const MAXIMUM_AUDIO_KBPS: u32 = 512;

pub fn available_heights(source: SourceFormat) -> Vec<u32> {
    let mut heights = vec![source.height];
    heights.extend(
        RESOLUTION_LADDER
            .iter()
            .copied()
            .filter(|height| *height < source.height),
    );
    heights
}

pub fn available_frame_rates(source: SourceFormat) -> Vec<u32> {
    let source_rate = nominal_frame_rate(source.frame_rate);
    let mut rates = vec![source_rate];
    rates.extend(
        FRAME_RATE_LADDER
            .iter()
            .copied()
            .filter(|rate| *rate < source_rate),
    );
    rates
}

pub fn scaled_width(source: SourceFormat, height: u32) -> u32 {
    let width = (f64::from(source.width) * f64::from(height) / f64::from(source.height.max(1)))
        .round() as u32;
    (width.max(2) + 1) & !1
}

fn pixel_rate(width: u32, height: u32, frame_rate: u32) -> f64 {
    f64::from(width) * f64::from(height) * f64::from(frame_rate)
}

fn pixel_based_kbps(source: SourceFormat, height: u32, frame_rate: u32, codec: VideoCodec) -> u32 {
    let pixels_per_second = pixel_rate(scaled_width(source, height), height, frame_rate);
    let kbps = pixels_per_second * codec.bits_per_pixel() / 1000.0;
    (kbps.round() as u32).clamp(MINIMUM_VIDEO_KBPS, MAXIMUM_VIDEO_KBPS)
}

pub fn convert_audio_kbps(kbps: u32, from: AudioChoice, to: AudioChoice) -> u32 {
    let converted = match (from, to) {
        (AudioChoice::Aac, AudioChoice::Opus) => (kbps * 3).div_ceil(4),
        (AudioChoice::Opus, AudioChoice::Aac) => (kbps * 4).div_ceil(3),
        _ => kbps,
    };
    AUDIO_BITRATE_STEPS
        .into_iter()
        .find(|step| *step >= converted)
        .unwrap_or(AUDIO_BITRATE_STEPS[AUDIO_BITRATE_STEPS.len() - 1])
}

impl ExportOptions {
    pub fn for_source(source: SourceFormat, streams: SourceStreams) -> Self {
        let mut options = Self {
            height: source.height,
            frame_rate: nominal_frame_rate(source.frame_rate).max(1),
            codec: streams.video_codec,
            audio: streams.audio,
            video_kbps: streams.video_kbps,
            audio_kbps: streams.audio_kbps,
            video_kbps_edited: false,
            streams,
        };
        options.recompute_bitrate(source);
        options
    }

    pub fn select_audio(&mut self, audio: AudioChoice) {
        self.audio = audio;
        self.audio_kbps = if audio == AudioChoice::CopySource {
            self.streams.audio_kbps
        } else {
            convert_audio_kbps(self.streams.audio_kbps, self.streams.audio, audio)
        };
    }

    pub fn recompute_bitrate(&mut self, source: SourceFormat) {
        if self.video_kbps_edited {
            return;
        }
        let current = pixel_rate(
            scaled_width(source, self.height),
            self.height,
            self.frame_rate,
        );
        let original = pixel_rate(
            source.width,
            source.height,
            nominal_frame_rate(source.frame_rate).max(1),
        );
        let codec_ratio = self.codec.bits_per_pixel() / self.streams.video_codec.bits_per_pixel();
        let kbps = f64::from(self.streams.video_kbps) * current / original * codec_ratio;
        self.video_kbps = (kbps.round() as u32).clamp(MINIMUM_VIDEO_KBPS, MAXIMUM_VIDEO_KBPS);
    }

    pub fn estimated_bytes(&self, duration_seconds: f64) -> u64 {
        let audio_kbps = if self.audio == AudioChoice::CopySource {
            self.streams.audio_kbps
        } else {
            self.audio_kbps
        };
        (f64::from(self.video_kbps + audio_kbps) * 1000.0 / 8.0 * duration_seconds.max(0.0)) as u64
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

    const STREAMS: SourceStreams = SourceStreams {
        video_codec: VideoCodec::Hevc,
        video_kbps: 30_000,
        audio: AudioChoice::Aac,
        audio_kbps: 128,
    };

    #[test]
    fn ladders_only_offer_values_not_above_the_source() {
        assert_eq!(available_heights(SOURCE), vec![1440, 1080, 720, 480]);
        assert_eq!(available_frame_rates(SOURCE), vec![60, 30, 24]);
        let small = SourceFormat {
            width: 1280,
            height: 720,
            frame_rate: 30.0,
        };
        assert_eq!(available_heights(small), vec![720, 480]);
        assert_eq!(available_frame_rates(small), vec![30, 24]);
    }

    #[test]
    fn options_start_with_the_original_clip_values() {
        let options = ExportOptions::for_source(SOURCE, STREAMS);
        assert_eq!(
            (
                options.height,
                options.frame_rate,
                options.codec,
                options.audio
            ),
            (1440, 60, VideoCodec::Hevc, AudioChoice::Aac)
        );
        assert_eq!((options.video_kbps, options.audio_kbps), (30_000, 128));
    }

    #[test]
    fn video_bitrate_follows_pixels_and_codec_efficiency() {
        let mut options = ExportOptions::for_source(SOURCE, STREAMS);
        options.height = 720;
        options.recompute_bitrate(SOURCE);
        assert_eq!(options.video_kbps, 7_500);
        options.height = 1440;
        options.codec = VideoCodec::H264;
        options.recompute_bitrate(SOURCE);
        assert_eq!(options.video_kbps, 46_667);
    }

    #[test]
    fn edited_bitrate_is_kept_when_other_options_change() {
        let mut options = ExportOptions::for_source(SOURCE, STREAMS);
        options.video_kbps = 5_000;
        options.video_kbps_edited = true;
        options.height = 720;
        options.recompute_bitrate(SOURCE);
        assert_eq!(options.video_kbps, 5_000);
    }

    #[test]
    fn audio_bitrates_convert_between_codecs_and_round_up_to_a_step() {
        assert_eq!(
            convert_audio_kbps(128, AudioChoice::Aac, AudioChoice::Opus),
            96
        );
        assert_eq!(
            convert_audio_kbps(96, AudioChoice::Opus, AudioChoice::Aac),
            128
        );
        assert_eq!(
            convert_audio_kbps(160, AudioChoice::Aac, AudioChoice::Opus),
            128
        );
        assert_eq!(
            convert_audio_kbps(100, AudioChoice::Aac, AudioChoice::Aac),
            128
        );
        assert_eq!(
            convert_audio_kbps(900, AudioChoice::Aac, AudioChoice::Aac),
            320
        );
    }

    #[test]
    fn switching_audio_codec_converts_from_the_original_each_time() {
        let mut options = ExportOptions::for_source(SOURCE, STREAMS);
        options.select_audio(AudioChoice::Opus);
        assert_eq!(options.audio_kbps, 96);
        options.select_audio(AudioChoice::Aac);
        assert_eq!(options.audio_kbps, 128);
        options.select_audio(AudioChoice::CopySource);
        assert_eq!(options.audio_kbps, 128);
    }

    #[test]
    fn widths_stay_even_and_keep_the_aspect_ratio() {
        assert_eq!(scaled_width(SOURCE, 720), 1280);
        assert_eq!(
            scaled_width(
                SourceFormat {
                    width: 1920,
                    height: 1080,
                    frame_rate: 60.0
                },
                480
            ),
            854
        );
    }
}
