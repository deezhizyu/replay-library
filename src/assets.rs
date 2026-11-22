use std::borrow::Cow;

use anyhow::Result;
use gpui::{App, AssetSource, SharedString};

pub struct EmbeddedAssets;

impl AssetSource for EmbeddedAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            "icons/search.svg" => include_bytes!("../assets/icons/search.svg"),
            "icons/settings.svg" => include_bytes!("../assets/icons/settings.svg"),
            "icons/chevron-down.svg" => include_bytes!("../assets/icons/chevron-down.svg"),
            "icons/arrow-left.svg" => include_bytes!("../assets/icons/arrow-left.svg"),
            "icons/minimize.svg" => include_bytes!("../assets/icons/minimize.svg"),
            "icons/maximize.svg" => include_bytes!("../assets/icons/maximize.svg"),
            "icons/close.svg" => include_bytes!("../assets/icons/close.svg"),
            "icons/play.svg" => include_bytes!("../assets/icons/play.svg"),
            "icons/pause.svg" => include_bytes!("../assets/icons/pause.svg"),
            "icons/skip-back.svg" => include_bytes!("../assets/icons/skip-back.svg"),
            "icons/skip-forward.svg" => include_bytes!("../assets/icons/skip-forward.svg"),
            "icons/volume.svg" => include_bytes!("../assets/icons/volume.svg"),
            "icons/volume-x.svg" => include_bytes!("../assets/icons/volume-x.svg"),
            "icons/fullscreen.svg" => include_bytes!("../assets/icons/fullscreen.svg"),
            "icons/zap.svg" => include_bytes!("../assets/icons/zap.svg"),
            "icons/scissors.svg" => include_bytes!("../assets/icons/scissors.svg"),
            "icons/file.svg" => include_bytes!("../assets/icons/file.svg"),
            "icons/folder.svg" => include_bytes!("../assets/icons/folder.svg"),
            "icons/trash.svg" => include_bytes!("../assets/icons/trash.svg"),
            "icons/plus.svg" => include_bytes!("../assets/icons/plus.svg"),
            "icons/refresh.svg" => include_bytes!("../assets/icons/refresh.svg"),
            "icons/download.svg" => include_bytes!("../assets/icons/download.svg"),
            "icons/repeat.svg" => include_bytes!("../assets/icons/repeat.svg"),
            "icons/check.svg" => include_bytes!("../assets/icons/check.svg"),
            "icons/cpu.svg" => include_bytes!("../assets/icons/cpu.svg"),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

pub fn register_fonts(cx: &App) -> Result<()> {
    cx.text_system().add_fonts(vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Medium.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-SemiBold.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf").as_slice()),
    ])
}
