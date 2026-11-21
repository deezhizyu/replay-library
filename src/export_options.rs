use crate::library::nominal_frame_rate;

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

    fn bits_per_pixel(self) -> f64 {
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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceFormat {
    pub width: u32,
    pub height: u32,
    pub frame_rate: f32,
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
}

const RESOLUTION_LADDER: [u32; 4] = [1440, 1080, 720, 480];
const FRAME_RATE_LADDER: [u32; 2] = [30, 24];
const DEFAULT_AUDIO_KBPS: u32 = 160;
const MINIMUM_VIDEO_KBPS: u32 = 300;
const MAXIMUM_VIDEO_KBPS: u32 = 120_000;

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

impl ExportOptions {
    pub fn for_source(source: SourceFormat) -> Self {
        let mut options = Self {
            height: source.height,
            frame_rate: nominal_frame_rate(source.frame_rate).max(1),
            codec: VideoCodec::Hevc,
            audio: AudioChoice::Aac,
            video_kbps: 0,
            audio_kbps: DEFAULT_AUDIO_KBPS,
            video_kbps_edited: false,
        };
        options.recompute_bitrate(source);
        options
    }

    pub fn recompute_bitrate(&mut self, source: SourceFormat) {
        if self.video_kbps_edited {
            return;
        }
        let width = f64::from(scaled_width(source, self.height));
        let pixels_per_second = width * f64::from(self.height) * f64::from(self.frame_rate);
        let kbps = pixels_per_second * self.codec.bits_per_pixel() / 1000.0;
        self.video_kbps = (kbps.round() as u32).clamp(MINIMUM_VIDEO_KBPS, MAXIMUM_VIDEO_KBPS);
    }

    pub fn estimated_bytes(&self, duration_seconds: f64) -> u64 {
        let audio_kbps = if self.audio == AudioChoice::CopySource {
            DEFAULT_AUDIO_KBPS
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
    fn default_bitrate_matches_the_design_for_1440p60_hevc() {
        let options = ExportOptions::for_source(SOURCE);
        assert_eq!(options.video_kbps, 19_907);
        assert_eq!(options.estimated_bytes(19.5), 48_913_312);
    }

    #[test]
    fn edited_bitrate_is_kept_when_other_options_change() {
        let mut options = ExportOptions::for_source(SOURCE);
        options.video_kbps = 5_000;
        options.video_kbps_edited = true;
        options.height = 720;
        options.recompute_bitrate(SOURCE);
        assert_eq!(options.video_kbps, 5_000);
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
