use std::path::Path;
use std::thread;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Context, Entity, Focusable as _, IntoElement, Render, SharedString, Task, Window, div, px,
};

use crate::components::titlebar::titlebar;
use crate::library::{Clip, ClipInfo};
use crate::library_model::LibraryModel;
use crate::media::source::VideoSource;
use crate::recycle_bin::move_to_recycle_bin;
use crate::theme;
use crate::views::library_view::{LibraryEvent, LibraryView};
use crate::views::player_view::{PlayerEvent, PlayerView};
use crate::views::settings_view::{SettingsEvent, SettingsView};

const DELETE_ATTEMPTS: u32 = 20;
const DELETE_RETRY_DELAY: Duration = Duration::from_millis(100);
const NOTICE_DURATION: Duration = Duration::from_secs(6);

enum Screen {
    Library,
    Settings(Entity<SettingsView>),
    Player(Entity<PlayerView>),
}

pub struct AppView {
    screen: Screen,
    library: Entity<LibraryModel>,
    library_view: Entity<LibraryView>,
    notice: Option<SharedString>,
    notice_timer: Option<Task<()>>,
}

impl AppView {
    pub fn new(library: Entity<LibraryModel>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let library_view = cx.new(|cx| LibraryView::new(library.clone(), cx));
        cx.subscribe_in(
            &library_view,
            window,
            |this, _, event: &LibraryEvent, window, cx| match event {
                LibraryEvent::OpenClip(index) => this.open_clip(*index, window, cx),
                LibraryEvent::OpenSettings => this.open_settings(window, cx),
            },
        )
        .detach();
        Self {
            screen: Screen::Library,
            library,
            library_view,
            notice: None,
            notice_timer: None,
        }
    }

    fn show_notice(&mut self, message: String, cx: &mut Context<Self>) {
        self.notice = Some(message.into());
        self.notice_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NOTICE_DURATION).await;
            this.update(cx, |this, cx| {
                this.notice = None;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    fn open_clip(&mut self, clip_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(clip) = self.library.read(cx).clips.get(clip_index).cloned() else {
            return;
        };
        if let Some(info) = clip.info.clone() {
            self.show_player(clip, info, window, cx);
            return;
        }
        let path = clip.path.clone();
        cx.spawn_in(window, async move |this, cx| {
            let probed = cx
                .background_executor()
                .spawn(async move { VideoSource::open(&path).map(|source| source.info()) })
                .await;
            this.update_in(cx, |this, window, cx| match probed {
                Ok(info) => this.show_player(clip, info, window, cx),
                Err(_) => {
                    let message = format!(
                        "Could not open {}. The file may be damaged or unsupported.",
                        clip.file_name()
                    );
                    this.show_notice(message, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn show_player(
        &mut self,
        clip: Clip,
        info: ClipInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let player = cx.new(|cx| PlayerView::new(clip, info, window, cx));
        cx.subscribe_in(
            &player,
            window,
            |this, _, event: &PlayerEvent, window, cx| match event {
                PlayerEvent::Back => this.close_player(window, cx),
                PlayerEvent::Delete => this.delete_clip(window, cx),
                PlayerEvent::Navigate(direction) => this.navigate(*direction, window, cx),
            },
        )
        .detach();
        self.screen = Screen::Player(player);
        cx.notify();
    }

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = cx.new(|cx| SettingsView::new(self.library.clone(), cx));
        cx.subscribe_in(
            &settings,
            window,
            |this, _, event: &SettingsEvent, window, cx| match event {
                SettingsEvent::Back => this.close_player(window, cx),
            },
        )
        .detach();
        self.screen = Screen::Settings(settings);
        cx.notify();
    }

    fn close_player(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Screen::Player(player) = &self.screen {
            let clip_id = player.read(cx).clip().id.clone();
            self.library_view
                .update(cx, |view, cx| view.set_last_viewed(clip_id, cx));
        }
        self.screen = Screen::Library;
        window.focus(&self.library_view.read(cx).focus_handle(cx));
        cx.notify();
    }

    fn delete_clip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Screen::Player(player) = &self.screen else {
            return;
        };
        let clip = player.read(cx).clip().clone();
        self.close_player(window, cx);
        cx.spawn(async move |this, cx| {
            let path = clip.path.clone();
            let outcome = cx
                .background_executor()
                .spawn(async move { move_to_recycle_bin_when_released(&path) })
                .await;
            this.update(cx, |this, cx| {
                if let Err(error) = outcome {
                    eprintln!("{error:#}");
                    let message = format!(
                        "Could not move {} to the Recycle Bin. Close any program that is using it and try again.",
                        clip.file_name()
                    );
                    this.show_notice(message, cx);
                }
                this.library.update(cx, |model, cx| model.rescan(cx))
            })
            .ok();
        })
        .detach();
    }

    fn navigate(&mut self, direction: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Screen::Player(player) = &self.screen else {
            return;
        };
        let current = player.read(cx).clip().id.clone();
        let model = self.library.read(cx);
        let Some(position) = model
            .order
            .iter()
            .position(|index| model.clips[*index].id == current)
        else {
            return;
        };
        let target = position as isize + direction;
        if let Some(clip_index) = usize::try_from(target)
            .ok()
            .and_then(|target| model.order.get(target).copied())
        {
            self.open_clip(clip_index, window, cx);
        }
    }
}

fn move_to_recycle_bin_when_released(path: &Path) -> anyhow::Result<()> {
    let mut attempts = 0;
    loop {
        match move_to_recycle_bin(path) {
            Err(_) if attempts < DELETE_ATTEMPTS => {
                attempts += 1;
                thread::sleep(DELETE_RETRY_DELAY);
            }
            outcome => return outcome,
        }
    }
}

impl Render for AppView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(theme::background())
            .text_color(theme::text())
            .font_family(theme::SANS_FONT)
            .border_1()
            .border_color(theme::border_subtle())
            .rounded(px(10.0))
            .relative()
            .child(titlebar())
            .child(div().flex_1().min_h_0().child(match &self.screen {
                Screen::Library => self.library_view.clone().into_any_element(),
                Screen::Player(player) => player.clone().into_any_element(),
                Screen::Settings(settings) => settings.clone().into_any_element(),
            }))
            .children(self.notice.clone().map(|message| {
                div()
                    .absolute()
                    .bottom(px(24.0))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .px_4()
                            .py_3()
                            .rounded(px(10.0))
                            .bg(theme::surface())
                            .border_1()
                            .border_color(theme::border())
                            .shadow_lg()
                            .text_size(px(13.0))
                            .child(message),
                    )
            }))
    }
}
