use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use ffmpeg::format::Pixel;
use ffmpeg::software::scaling::Flags;
use ffmpeg_next as ffmpeg;
use gpui::RenderImage;

use crate::media::convert::{FrameConverter, fit_within};
use crate::media::render_image::from_bgra;
use crate::media::source::VideoSource;

pub const TILE_COUNT: usize = 28;
const TILE_WIDTH: u32 = 192;
const TILE_HEIGHT: u32 = 108;

#[derive(Clone)]
pub struct Filmstrip {
    tiles: Arc<Mutex<Vec<Option<Arc<RenderImage>>>>>,
}

impl Filmstrip {
    pub fn start(path: PathBuf, duration_seconds: f64) -> Self {
        let tiles = Arc::new(Mutex::new(vec![None; TILE_COUNT]));
        thread::spawn({
            let tiles = tiles.clone();
            move || fill(&path, duration_seconds, &tiles)
        });
        Self { tiles }
    }

    pub fn tiles(&self) -> Vec<Option<Arc<RenderImage>>> {
        self.tiles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn tile_at(&self, fraction: f64) -> Option<Arc<RenderImage>> {
        let tiles = self.tiles();
        let index = ((fraction.clamp(0.0, 1.0) * TILE_COUNT as f64) as usize).min(TILE_COUNT - 1);
        tiles[index]
            .clone()
            .or_else(|| tiles.iter().flatten().next().cloned())
    }

    pub fn complete(&self) -> bool {
        self.tiles().iter().all(Option::is_some)
    }
}

fn fill(path: &Path, duration_seconds: f64, tiles: &Mutex<Vec<Option<Arc<RenderImage>>>>) {
    let Ok(mut source) = VideoSource::open(path) else {
        return;
    };
    let mut converter = FrameConverter::new(Pixel::BGRA, Flags::BILINEAR);
    for index in 0..TILE_COUNT {
        let position = duration_seconds * (index as f64 + 0.5) / TILE_COUNT as f64;
        if source.seek_seconds(position).is_err() {
            continue;
        }
        let Ok(Some(decoded)) = source.next_frame() else {
            continue;
        };
        let (width, height) =
            fit_within(decoded.width(), decoded.height(), TILE_WIDTH, TILE_HEIGHT);
        let image = converter
            .convert_packed(&decoded, width, height)
            .and_then(|bytes| from_bgra(width, height, bytes));
        if let Ok(image) = image {
            tiles
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())[index] = Some(image);
        }
    }
}
