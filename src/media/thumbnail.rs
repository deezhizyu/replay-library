use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result};
use ffmpeg::format::Pixel;
use ffmpeg::software::scaling::Flags;
use ffmpeg_next as ffmpeg;
use image::codecs::jpeg::JpegEncoder;

use crate::library::ClipInfo;
use crate::media::convert::{FrameConverter, fit_within};
use crate::media::source::VideoSource;

const THUMBNAIL_WIDTH: u32 = 320;
const POSTER_POSITION: f64 = 0.15;
const JPEG_QUALITY: u8 = 82;

pub fn probe_and_write_thumbnail(clip: &Path, destination: &Path) -> Result<ClipInfo> {
    let mut source = VideoSource::open(clip)?;
    let info = source.info();
    if !destination.exists() {
        source.seek_seconds(info.duration_seconds * POSTER_POSITION)?;
        let decoded = source
            .next_frame()?
            .context("no frame to use as a thumbnail")?;
        let (width, height) = fit_within(
            decoded.width(),
            decoded.height(),
            THUMBNAIL_WIDTH,
            THUMBNAIL_WIDTH,
        );
        let packed = FrameConverter::new(Pixel::RGB24, Flags::BILINEAR)
            .convert_packed(&decoded, width, height)?;
        write_jpeg(&packed, width, height, destination)?;
    }
    Ok(info)
}

fn write_jpeg(packed: &[u8], width: u32, height: u32, destination: &Path) -> Result<()> {
    if let Some(directory) = destination.parent() {
        fs::create_dir_all(directory)?;
    }
    let temporary = destination.with_extension("tmp");
    let file = fs::File::create(&temporary)?;
    JpegEncoder::new_with_quality(file, JPEG_QUALITY).encode(
        packed,
        width,
        height,
        image::ExtendedColorType::Rgb8,
    )?;
    fs::rename(&temporary, destination)?;
    Ok(())
}
