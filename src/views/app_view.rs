use std::path::Path;
use std::thread;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{Context, Entity, Focusable as _, IntoElement, Render, SharedString, Window, div, px};

use crate::components::titlebar::titlebar;
use crate::library_model::LibraryModel;
use crate::media::source::VideoSource;
use crate::recycle_bin::move_to_recycle_bin;
use crate::theme;
use crate::views::library_view::{LibraryEvent, LibraryView};
use crate::views::player_view::{PlayerEvent, PlayerView};
use crate::views::settings_view::{SettingsEvent, SettingsView};

const DELETE_ATTEMPTS: u32 = 20;
const DELETE_RETRY_DELAY: Duration = Duration::from_millis(100);

enum Screen {
    Library,
    Settings(Entity<SettingsView>),
    Player(Entity<PlayerView>),
}

pub struct AppView {
    screen: Screen,
    library: Entity<LibraryModel>,
    library_view: Entity<LibraryView>,
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
        }
    }

    fn open_clip(&mut self, clip_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(clip) = self.library.read(cx).clips.get(clip_index).cloned() else {
            return;
        };
        let Some(info) = clip.info.clone().or_else(|| {
            VideoSource::open(&clip.path)
                .ok()
                .map(|source| source.info())
        }) else {
            return;
        };
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
        self.screen = Screen::Library;
        window.focus(&self.library_view.read(cx).focus_handle(cx));
        cx.notify();
    }

    fn delete_clip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Screen::Player(player) = &self.screen else {
            return;
        };
        let path = player.read(cx).clip().path.clone();
        self.close_player(window, cx);
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move { move_to_recycle_bin_when_released(&path) })
                .await;
            if let Err(error) = outcome {
                eprintln!("{error:#}");
            }
            this.update(cx, |this, cx| {
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let breadcrumb = match &self.screen {
            Screen::Library => None,
            Screen::Settings(_) => Some(SharedString::from("Settings")),
            Screen::Player(player) => Some(SharedString::from(player.read(cx).clip().file_name())),
        };
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
            .child(titlebar(breadcrumb))
            .child(div().flex_1().min_h_0().child(match &self.screen {
                Screen::Library => self.library_view.clone().into_any_element(),
                Screen::Player(player) => player.clone().into_any_element(),
                Screen::Settings(settings) => settings.clone().into_any_element(),
            }))
    }
}
