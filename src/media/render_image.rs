use std::sync::Arc;

use anyhow::{Result, anyhow};
use gpui::RenderImage;
use image::{Frame, RgbaImage};

pub fn from_bgra(width: u32, height: u32, bytes: Vec<u8>) -> Result<Arc<RenderImage>> {
    let buffer = RgbaImage::from_raw(width, height, bytes)
        .ok_or_else(|| anyhow!("pixel buffer does not match {width}x{height}"))?;
    Ok(Arc::new(RenderImage::new(vec![Frame::new(buffer)])))
}
