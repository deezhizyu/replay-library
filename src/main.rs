#![cfg_attr(all(not(test), not(debug_assertions)), windows_subsystem = "windows")]

mod app_paths;
mod assets;
mod capture_folders;
mod clip_name;
mod components;
mod compress_plan;
mod export_options;
mod format;
mod indexer;
mod library;
mod library_model;
mod local_time;
mod media;
mod probe_cache;
mod recycle_bin;
mod settings;
mod theme;
mod transcode;
mod trim_selection;
mod views;

use gpui::{
    AppContext as _, Application, Bounds, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions,
    px, size,
};

use crate::assets::{EmbeddedAssets, register_fonts};
use crate::library_model::LibraryModel;
use crate::settings::Settings;
use crate::views::app_view::AppView;
use crate::views::library_view::FocusSearch;

const WINDOW_SIZE: (f32, f32) = (1440.0, 900.0);
const WINDOW_MINIMUM_SIZE: (f32, f32) = (960.0, 600.0);

fn main() -> anyhow::Result<()> {
    ffmpeg_next::init()?;
    Application::new().with_assets(EmbeddedAssets).run(|cx| {
        if let Err(error) = register_fonts(cx) {
            eprintln!("failed to register fonts: {error:#}");
        }
        let settings = Settings::load().unwrap_or_else(|| {
            let first_start = Settings {
                folders: capture_folders::detect(),
                ..Settings::default()
            };
            if let Err(error) = first_start.save() {
                eprintln!("failed to save the first-start settings: {error:#}");
            }
            first_start
        });
        let folders = settings.folders.clone();
        cx.set_global(settings);
        cx.set_global(transcode::EncoderCatalog::start());
        cx.bind_keys([KeyBinding::new("ctrl-k", FocusSearch, Some("LibraryView"))]);
        let bounds = Bounds::centered(None, size(px(WINDOW_SIZE.0), px(WINDOW_SIZE.1)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Replay Library".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            window_min_size: Some(size(px(WINDOW_MINIMUM_SIZE.0), px(WINDOW_MINIMUM_SIZE.1))),
            ..Default::default()
        };
        let opened = cx.open_window(options, |window, cx| {
            let library = cx.new(|cx| LibraryModel::new(folders, cx));
            cx.new(|cx| AppView::new(library, window, cx))
        });
        if let Err(error) = opened {
            eprintln!("failed to open the window: {error:#}");
            cx.quit();
        }
        cx.activate(true);
    });
    Ok(())
}
