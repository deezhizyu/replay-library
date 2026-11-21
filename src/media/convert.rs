use anyhow::Result;
use ffmpeg::color::{Range, Space};
use ffmpeg::format::Pixel;
use ffmpeg::frame;
use ffmpeg::software::scaling::{Context as Scaler, Flags};
use ffmpeg_next as ffmpeg;

const HIGH_DEFINITION_HEIGHT: u32 = 720;
const FIXED_POINT_ONE: i32 = 1 << 16;

#[derive(Clone, Copy, PartialEq, Eq)]
struct ScalerKey {
    format: Pixel,
    source_width: u32,
    source_height: u32,
    range: Range,
    space: Space,
    width: u32,
    height: u32,
}

pub struct FrameConverter {
    destination: Pixel,
    flags: Flags,
    scaler: Option<(ScalerKey, Scaler)>,
    output: frame::Video,
}

impl FrameConverter {
    pub fn new(destination: Pixel, flags: Flags) -> Self {
        Self {
            destination,
            flags,
            scaler: None,
            output: frame::Video::empty(),
        }
    }

    pub fn convert_packed(
        &mut self,
        source: &frame::Video,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>> {
        let bytes_per_pixel = if self.destination == Pixel::RGB24 {
            3
        } else {
            4
        };
        let output = self.convert(source, width, height)?;
        let row_bytes = width as usize * bytes_per_pixel;
        let stride = output.stride(0);
        let mut packed = Vec::with_capacity(row_bytes * height as usize);
        for row in output.data(0).chunks(stride).take(height as usize) {
            packed.extend_from_slice(&row[..row_bytes]);
        }
        Ok(packed)
    }

    fn convert(&mut self, source: &frame::Video, width: u32, height: u32) -> Result<&frame::Video> {
        let key = ScalerKey {
            format: source.format(),
            source_width: source.width(),
            source_height: source.height(),
            range: source.color_range(),
            space: source.color_space(),
            width,
            height,
        };
        if self
            .scaler
            .as_ref()
            .is_none_or(|(existing, _)| *existing != key)
        {
            let mut scaler = Scaler::get(
                key.format,
                key.source_width,
                key.source_height,
                self.destination,
                width,
                height,
                self.flags,
            )?;
            apply_color_details(&mut scaler, &key);
            self.output = frame::Video::empty();
            self.scaler = Some((key, scaler));
        }
        if let Some((_, scaler)) = self.scaler.as_mut() {
            scaler.run(source, &mut self.output)?;
        }
        Ok(&self.output)
    }
}

fn apply_color_details(scaler: &mut Scaler, key: &ScalerKey) {
    let table = match key.space {
        Space::BT709 => ffmpeg::ffi::SWS_CS_ITU709,
        Space::BT2020NCL | Space::BT2020CL => ffmpeg::ffi::SWS_CS_BT2020,
        Space::Unspecified if key.source_height >= HIGH_DEFINITION_HEIGHT => {
            ffmpeg::ffi::SWS_CS_ITU709
        }
        _ => ffmpeg::ffi::SWS_CS_DEFAULT,
    };
    let source_full_range = i32::from(key.range == Range::JPEG);
    unsafe {
        let coefficients = ffmpeg::ffi::sws_getCoefficients(table);
        ffmpeg::ffi::sws_setColorspaceDetails(
            scaler.as_mut_ptr(),
            coefficients,
            source_full_range,
            coefficients,
            1,
            0,
            FIXED_POINT_ONE,
            FIXED_POINT_ONE,
        );
    }
}

pub fn fit_within(
    source_width: u32,
    source_height: u32,
    box_width: u32,
    box_height: u32,
) -> (u32, u32) {
    if source_width == 0 || source_height == 0 || box_width == 0 || box_height == 0 {
        return (source_width.max(2) & !1, source_height.max(2) & !1);
    }
    let scale = (f64::from(box_width) / f64::from(source_width))
        .min(f64::from(box_height) / f64::from(source_height))
        .min(1.0);
    let width = ((f64::from(source_width) * scale).round() as u32).max(2) & !1;
    let height = ((f64::from(source_height) * scale).round() as u32).max(2) & !1;
    (width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_inside_the_box_without_upscaling() {
        assert_eq!(fit_within(2560, 1440, 1280, 1000), (1280, 720));
        assert_eq!(fit_within(1920, 1080, 3840, 2160), (1920, 1080));
        assert_eq!(fit_within(1080, 1920, 1000, 500), (280, 500));
    }

    #[test]
    fn degenerate_boxes_keep_an_even_source_size() {
        assert_eq!(fit_within(1921, 1081, 0, 0), (1920, 1080));
    }
}
