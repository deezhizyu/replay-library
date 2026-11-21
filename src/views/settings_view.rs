use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{
    Context, Entity, EventEmitter, FontWeight, IntoElement, PathPromptOptions, Render,
    SharedString, Window, div, px,
};

use crate::components::button::{ButtonKind, button};
use crate::components::icon::icon;
use crate::components::toggle::toggle;
use crate::format::{format_count, format_size};
use crate::library_model::LibraryModel;
use crate::settings::{FolderConfig, FolderKind, Settings};
use crate::theme;

pub enum SettingsEvent {
    Back,
}

pub struct SettingsView {
    library: Entity<LibraryModel>,
    cache_bytes: u64,
}

impl EventEmitter<SettingsEvent> for SettingsView {}

impl SettingsView {
    pub fn new(library: Entity<LibraryModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&library, |_, _, cx| cx.notify()).detach();
        let mut view = Self {
            library,
            cache_bytes: 0,
        };
        view.refresh_cache_size(cx);
        view
    }

    fn refresh_cache_size(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let bytes = cx
                .background_executor()
                .spawn(async { directory_size(&crate::app_paths::cache_directory()) })
                .await;
            this.update(cx, |this, cx| {
                this.cache_bytes = bytes;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn update_settings(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Settings)) {
        cx.update_global::<Settings, _>(|settings, _| change(settings));
        let settings = cx.global::<Settings>().clone();
        if let Err(error) = settings.save() {
            eprintln!("failed to save the settings: {error:#}");
        }
        self.library.update(cx, |library, cx| {
            library.set_folders(settings.folders, settings.watch_for_new_clips, cx)
        });
        cx.notify();
    }

    fn add_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            this.update(cx, |this, cx| {
                for path in paths {
                    this.update_settings(cx, |settings| {
                        if !settings.folders.iter().any(|folder| folder.path == path) {
                            settings.folders.push(FolderConfig {
                                path,
                                enabled: true,
                                kind: FolderKind::Other,
                            });
                        }
                    });
                }
            })
            .ok();
        })
        .detach();
    }

    fn folder_row(
        &self,
        index: usize,
        folder: &FolderConfig,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (count, bytes) = self.library.read(cx).folder_summary(folder);
        div()
            .h(px(68.0))
            .px_5()
            .flex()
            .items_center()
            .gap_4()
            .border_b_1()
            .border_color(theme::border_subtle())
            .child(
                div()
                    .size(px(36.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.0))
                    .bg(theme::surface_raised())
                    .child(icon("folder", 18.0, theme::text())),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(theme::MONO_FONT)
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(
                                folder.path.to_string_lossy().into_owned(),
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::text_muted())
                            .child(SharedString::from(format!(
                                "{} videos · {}",
                                format_count(count),
                                format_size(bytes)
                            ))),
                    ),
            )
            .child(
                div()
                    .h(px(28.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .rounded_full()
                    .border_1()
                    .border_color(theme::border())
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(folder.kind.label()),
            )
            .child(
                toggle(("folder-enabled", index), folder.enabled).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.update_settings(cx, |settings| {
                            settings.folders[index].enabled = !settings.folders[index].enabled
                        });
                    },
                )),
            )
            .child(
                div()
                    .id(("folder-remove", index))
                    .size(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::surface_hover()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.update_settings(cx, |settings| {
                            settings.folders.remove(index);
                        });
                    }))
                    .child(icon("trash", 16.0, theme::text_muted())),
            )
    }

    fn setting_row(
        &self,
        id: &'static str,
        title: &'static str,
        detail: &'static str,
        enabled: bool,
        cx: &mut Context<Self>,
        change: fn(&mut Settings),
    ) -> impl IntoElement + use<> {
        div()
            .h(px(68.0))
            .px_5()
            .flex()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(theme::border_subtle())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::text_muted())
                            .child(detail),
                    ),
            )
            .child(
                toggle(id, enabled)
                    .on_click(cx.listener(move |this, _, _, cx| this.update_settings(cx, change))),
            )
    }
}

fn directory_size(directory: &PathBuf) -> u64 {
    std::fs::read_dir(directory)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| match entry.metadata() {
                    Ok(metadata) if metadata.is_dir() => directory_size(&entry.path()),
                    Ok(metadata) => metadata.len(),
                    Err(_) => 0,
                })
                .sum()
        })
        .unwrap_or(0)
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = cx.global::<Settings>().clone();
        let rows: Vec<_> = settings
            .folders
            .iter()
            .enumerate()
            .map(|(index, folder)| self.folder_row(index, folder, cx))
            .collect();
        div()
            .size_full()
            .flex()
            .child(
                div()
                    .w(px(264.0))
                    .flex_none()
                    .h_full()
                    .p_3()
                    .pt_5()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_r_1()
                    .border_color(theme::border_subtle())
                    .child(
                        div()
                            .id("settings-back")
                            .h(px(40.0))
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::MEDIUM)
                            .cursor_pointer()
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(SettingsEvent::Back)))
                            .child(icon("arrow-left", 16.0, theme::text()))
                            .child("Library"),
                    )
                    .child(
                        div()
                            .h(px(44.0))
                            .px_3()
                            .mt_2()
                            .flex()
                            .items_center()
                            .gap_3()
                            .rounded(px(10.0))
                            .bg(theme::surface_raised())
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(icon("folder", 18.0, theme::accent()))
                            .child("Folders"),
                    ),
            )
            .child(
                div()
                    .id("settings-content")
                    .flex_1()
                    .h_full()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w(px(780.0))
                            .mx_auto()
                            .pt_10()
                            .pb_10()
                            .flex()
                            .flex_col()
                            .gap_5()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().text_size(px(28.0)).font_weight(FontWeight::SEMIBOLD).child("Video folders"))
                                    .child(div().text_size(px(14.0)).text_color(theme::text_muted()).child("Replay Library watches these folders and indexes new clips the moment they land.")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .rounded(px(14.0))
                                    .border_1()
                                    .border_color(theme::border())
                                    .bg(theme::surface())
                                    .children(rows)
                                    .child(
                                        div()
                                            .id("add-folder")
                                            .h(px(68.0))
                                            .m_2()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .gap_2()
                                            .rounded(px(10.0))
                                            .border_1()
                                            .border_dashed()
                                            .border_color(theme::border_strong())
                                            .text_size(px(13.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .cursor_pointer()
                                            .hover(|style| style.bg(theme::surface_hover()))
                                            .on_click(cx.listener(|this, _, window, cx| this.add_folder(window, cx)))
                                            .child(icon("plus", 16.0, theme::text()))
                                            .child("Add folder…"),
                                    ),
                            )
                            .child(div().text_size(px(16.0)).font_weight(FontWeight::SEMIBOLD).child("Indexing"))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .rounded(px(14.0))
                                    .border_1()
                                    .border_color(theme::border())
                                    .bg(theme::surface())
                                    .child(self.setting_row("watch-toggle", "Watch for new clips", "Index files the moment they finish writing.", settings.watch_for_new_clips, cx, |settings| {
                                        settings.watch_for_new_clips = !settings.watch_for_new_clips
                                    }))
                                    .child(self.setting_row("hover-toggle", "Hover-scrub previews", "Keep a small frame strip per video so the grid scrubs instantly.", settings.hover_scrub_previews, cx, |settings| {
                                        settings.hover_scrub_previews = !settings.hover_scrub_previews
                                    }))
                                    .child(
                                        div()
                                            .h(px(68.0))
                                            .px_5()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .child(div().text_size(px(14.0)).font_weight(FontWeight::SEMIBOLD).child("Thumbnail cache"))
                                                    .child(div().font_family(theme::MONO_FONT).text_size(px(12.0)).text_color(theme::text_muted()).child(SharedString::from(format!("{} on disk", format_size(self.cache_bytes))))),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .gap_3()
                                                    .child(button("rebuild-cache", "Rebuild", Some("refresh"), ButtonKind::Secondary).on_click(cx.listener(|this, _, _, cx| {
                                                        this.library.update(cx, |library, cx| library.rebuild_thumbnails(cx));
                                                        this.refresh_cache_size(cx);
                                                    })))
                                                    .child(button("clear-cache", "Clear", None, ButtonKind::Secondary).on_click(cx.listener(|this, _, _, cx| {
                                                        this.library.update(cx, |library, cx| library.clear_cache(cx));
                                                        this.refresh_cache_size(cx);
                                                    }))),
                                            ),
                                    ),
                            ),
                    ),
            )
    }
}
