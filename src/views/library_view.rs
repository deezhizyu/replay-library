use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    IntoElement, ListAlignment, ListState, MouseMoveEvent, ObjectFit, Render, RenderImage,
    SharedString, Window, actions, deferred, div, img, list, px,
};

use crate::components::icon::icon;
use crate::components::slider::slider;
use crate::components::text_field::{TextChanged, TextField};
use crate::format::{format_count, format_duration, format_size};
use crate::library::{Clip, SortOrder};
use crate::library_model::LibraryModel;
use crate::media::filmstrip::{Filmstrip, HOVER_TILE_COUNT};
use crate::settings::Settings;
use crate::theme;
use std::sync::Arc;

actions!(library, [FocusSearch]);

const PAGE_PADDING: f32 = 32.0;
const CARD_GAP: f32 = 16.0;
const MINIMUM_CARD_WIDTH_RANGE: (f32, f32) = (180.0, 360.0);
const TEXT_BLOCK_HEIGHT: f32 = 46.0;
const CHROME_HEIGHT: f32 = 44.0 + 64.0 + 30.0 + 16.0;
const SCROLLBAR_MINIMUM_THUMB: f32 = 40.0;

pub enum LibraryEvent {
    OpenClip(usize),
    OpenSettings,
}

struct HoverState {
    clip_index: usize,
    fraction: f32,
    strip: Option<Filmstrip>,
}

enum Row {
    Header(usize),
    Cards(Vec<usize>),
}

pub struct LibraryView {
    model: Entity<LibraryModel>,
    search: Entity<TextField>,
    list_state: ListState,
    rows: Vec<Row>,
    rows_key: (u64, usize),
    card_width: f32,
    sort_menu_open: bool,
    hover: Option<HoverState>,
    focus_handle: FocusHandle,
}

impl EventEmitter<LibraryEvent> for LibraryView {}

impl Focusable for LibraryView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl LibraryView {
    pub fn new(model: Entity<LibraryModel>, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| TextField::new("Search videos", false, cx));
        cx.subscribe(&search, |this, _, TextChanged(query): &TextChanged, cx| {
            this.model
                .update(cx, |model, cx| model.set_query(query.clone(), cx));
        })
        .detach();
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        Self {
            model,
            search,
            list_state: ListState::new(0, ListAlignment::Top, px(600.0)),
            rows: Vec::new(),
            rows_key: (u64::MAX, 0),
            card_width: 260.0,
            sort_menu_open: false,
            hover: None,
            focus_handle: cx.focus_handle(),
        }
    }

    fn hover_move(
        &mut self,
        clip_index: usize,
        column: usize,
        pointer_x: f32,
        cx: &mut Context<Self>,
    ) {
        if !cx.global::<Settings>().hover_scrub_previews {
            return;
        }
        let left = PAGE_PADDING + column as f32 * (self.card_width + CARD_GAP);
        let fraction = ((pointer_x - left) / self.card_width).clamp(0.0, 1.0);
        let tile_of = |fraction: f32| (fraction * HOVER_TILE_COUNT as f32) as usize;
        let unchanged = self.hover.as_ref().is_some_and(|hover| {
            hover.clip_index == clip_index && tile_of(hover.fraction) == tile_of(fraction)
        });
        if unchanged {
            return;
        }
        let strip = match self.hover.take() {
            Some(HoverState {
                clip_index: previous,
                strip,
                ..
            }) if previous == clip_index && strip.is_some() => strip,
            _ => self
                .model
                .update(cx, |model, _| model.hover_strip(clip_index)),
        };
        self.hover = Some(HoverState {
            clip_index,
            fraction,
            strip,
        });
        cx.notify();
    }

    fn columns(&self, window: &Window, thumbnail_size: f32) -> usize {
        let available = f32::from(window.viewport_size().width) - PAGE_PADDING * 2.0;
        let (smallest, largest) = MINIMUM_CARD_WIDTH_RANGE;
        let minimum_width = smallest + (largest - smallest) * thumbnail_size;
        (((available + CARD_GAP) / (minimum_width + CARD_GAP)).floor() as usize).max(1)
    }

    fn rebuild_rows(&mut self, columns: usize, cx: &App) {
        let model = self.model.read(cx);
        self.rows.clear();
        for (group_index, group) in model.groups.iter().enumerate() {
            self.rows.push(Row::Header(group_index));
            let positions: Vec<usize> = group.members.clone().collect();
            for chunk in positions.chunks(columns) {
                self.rows.push(Row::Cards(chunk.to_vec()));
            }
        }
        self.list_state.reset(self.rows.len());
        self.rows_key = (model.revision, columns);
    }

    fn render_row(&mut self, index: usize, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match &self.rows[index] {
            Row::Header(group_index) => {
                let group = self.model.read(cx).groups[*group_index].clone();
                div()
                    .pt(px(20.0))
                    .pb(px(10.0))
                    .h(px(58.0))
                    .flex()
                    .items_baseline()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(group.label)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::text_muted())
                            .child(SharedString::from(group.caption)),
                    )
                    .into_any_element()
            }
            Row::Cards(positions) => {
                let entries: Vec<(usize, Clip, bool)> = {
                    let model = self.model.read(cx);
                    positions
                        .iter()
                        .map(|position| {
                            let clip_index = model.order[*position];
                            let clip = &model.clips[clip_index];
                            model.request_thumbnail(clip);
                            (clip_index, clip.clone(), model.thumbnail_ready(clip))
                        })
                        .collect()
                };
                let cards: Vec<AnyElement> = entries
                    .iter()
                    .enumerate()
                    .map(|(column, (clip_index, clip, ready))| {
                        self.card(*clip_index, column, clip, *ready, cx)
                    })
                    .collect();
                div()
                    .flex()
                    .gap(px(CARD_GAP))
                    .pb(px(CARD_GAP))
                    .children(cards)
                    .into_any_element()
            }
        }
    }

    fn card(
        &self,
        clip_index: usize,
        column: usize,
        clip: &Clip,
        thumbnail_ready: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = self
            .hover
            .as_ref()
            .filter(|hover| hover.clip_index == clip_index);
        let hover_fraction = hover.map(|hover| hover.fraction);
        let hover_tile: Option<Arc<RenderImage>> =
            hover.and_then(|hover| hover.strip.as_ref()?.tile_at(f64::from(hover.fraction)));
        let group_name = SharedString::from(format!("clip-{clip_index}"));
        let thumbnail_height = self.card_width * 9.0 / 16.0;
        let badge = |text: SharedString| {
            div()
                .h(px(22.0))
                .px(px(7.0))
                .flex()
                .items_center()
                .rounded(px(6.0))
                .bg(theme::badge_background())
                .font_family(theme::MONO_FONT)
                .text_size(px(11.0))
                .child(text)
        };
        let info = clip.info.as_ref();
        div()
            .id(("clip", clip_index))
            .group(group_name.clone())
            .w(px(self.card_width))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .cursor_pointer()
            .on_click(cx.listener(move |_, _, _, cx| cx.emit(LibraryEvent::OpenClip(clip_index))))
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                this.hover_move(clip_index, column, f32::from(event.position.x), cx);
            }))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if !*hovered
                    && this
                        .hover
                        .as_ref()
                        .is_some_and(|hover| hover.clip_index == clip_index)
                {
                    this.hover = None;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(thumbnail_height))
                    .rounded(px(10.0))
                    .overflow_hidden()
                    .bg(theme::surface())
                    .when(thumbnail_ready, |this| {
                        this.child(
                            img(clip.thumbnail_path())
                                .w_full()
                                .h_full()
                                .object_fit(ObjectFit::Cover),
                        )
                    })
                    .children(hover_tile.map(|image| {
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .child(img(image).size_full().object_fit(ObjectFit::Cover))
                    }))
                    .children(hover_fraction.map(|fraction| {
                        div()
                            .absolute()
                            .bottom_0()
                            .left_0()
                            .h(px(4.0))
                            .w(gpui::relative(fraction))
                            .bg(theme::accent())
                    }))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .rounded(px(10.0))
                            .border_2()
                            .border_color(gpui::transparent_black())
                            .group_hover(group_name.clone(), |style| {
                                style
                                    .border_color(theme::accent())
                                    .bg(gpui::rgba(0x0000002e))
                            }),
                    )
                    .children(info.map(|info| {
                        div()
                            .absolute()
                            .left(px(8.0))
                            .top(px(8.0))
                            .child(badge(info.badge().into()))
                    }))
                    .children(info.map(|info| {
                        div()
                            .absolute()
                            .right(px(8.0))
                            .bottom(px(10.0))
                            .child(badge(format_duration(info.duration_seconds).into()))
                    }))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .invisible()
                            .group_hover(group_name, |style| style.visible())
                            .child(
                                div()
                                    .size(px(44.0))
                                    .rounded_full()
                                    .bg(theme::accent())
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon("play", 18.0, theme::on_accent())),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(TEXT_BLOCK_HEIGHT - 10.0))
                    .px(px(2.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(clip.title.clone())),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::text_muted())
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(SharedString::from(clip.detail_line())),
                    ),
            )
            .into_any_element()
    }

    fn toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let model = self.model.read(cx);
        let sort = model.sort;
        let thumbnail_size = cx.global::<Settings>().thumbnail_size;
        let menu_open = self.sort_menu_open;
        div()
            .h(px(64.0))
            .flex_none()
            .flex()
            .items_center()
            .gap_4()
            .px(px(PAGE_PADDING))
            .border_b_1()
            .border_color(theme::border_subtle())
            .child(
                div()
                    .relative()
                    .w(px(340.0))
                    .h(px(40.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2p5()
                    .px(px(14.0))
                    .rounded(px(10.0))
                    .bg(theme::surface())
                    .border_1()
                    .border_color(theme::border())
                    .child(icon("search", 18.0, theme::text_dim()))
                    .child(div().flex_1().h_full().child(self.search.clone()))
                    .child(
                        div()
                            .h(px(22.0))
                            .px(px(7.0))
                            .flex()
                            .items_center()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(theme::border_strong())
                            .font_family(theme::MONO_FONT)
                            .text_size(px(11.0))
                            .text_color(theme::text_muted())
                            .child("Ctrl K"),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2p5()
                    .text_size(px(12.0))
                    .text_color(theme::text_muted())
                    .child("Size")
                    .child(slider(
                        "thumbnail-size",
                        thumbnail_size,
                        96.0,
                        window,
                        cx,
                        |fraction, cx| {
                            cx.update_global::<Settings, _>(|settings, _| {
                                settings.thumbnail_size = fraction
                            });
                            cx.refresh_windows();
                        },
                    )),
            )
            .child(
                div()
                    .relative()
                    .child(
                        div()
                            .id("sort-button")
                            .h(px(40.0))
                            .pl(px(14.0))
                            .pr(px(12.0))
                            .flex()
                            .items_center()
                            .gap_2()
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(theme::border())
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::surface_raised()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sort_menu_open = !this.sort_menu_open;
                                cx.notify();
                            }))
                            .child(sort.label())
                            .child(icon("chevron-down", 16.0, theme::text_muted())),
                    )
                    .when(menu_open, |this| {
                        this.child(deferred(self.sort_menu(sort, cx)).with_priority(1))
                    }),
            )
            .child(
                div()
                    .id("settings-button")
                    .size(px(44.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12.0))
                    .bg(theme::surface_raised())
                    .border_1()
                    .border_color(gpui::rgb(0x262b33))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::surface_hover()))
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(LibraryEvent::OpenSettings)))
                    .child(icon("settings", 20.0, theme::text())),
            )
    }

    fn sort_menu(&self, current: SortOrder, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .top(px(46.0))
            .right_0()
            .w(px(180.0))
            .p_1()
            .rounded(px(10.0))
            .bg(theme::surface_raised())
            .border_1()
            .border_color(theme::border_strong())
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.sort_menu_open = false;
                cx.notify();
            }))
            .children(SortOrder::ALL.map(|order| {
                div()
                    .id(SharedString::from(order.label()))
                    .h(px(32.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .rounded(px(7.0))
                    .text_size(px(13.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::surface_hover()))
                    .when(order == current, |this| this.text_color(theme::accent()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.sort_menu_open = false;
                        this.model.update(cx, |model, cx| model.set_sort(order, cx));
                    }))
                    .child(order.label())
            }))
    }

    fn scrollbar(&self, window: &Window) -> impl IntoElement + use<> {
        let viewport = f32::from(window.viewport_size().height) - CHROME_HEIGHT;
        let scrollable = f32::from(self.list_state.max_offset_for_scrollbar().height);
        let offset = f32::from(self.list_state.scroll_px_offset_for_scrollbar().y).abs();
        let visible = scrollable > 0.0 && viewport > 0.0;
        let thumb_height =
            (viewport * viewport / (viewport + scrollable)).max(SCROLLBAR_MINIMUM_THUMB);
        let thumb_top = (viewport - thumb_height) * (offset / scrollable.max(1.0)).clamp(0.0, 1.0);
        div().when(visible, |this| {
            this.absolute()
                .top(px(thumb_top + 8.0))
                .right(px(8.0))
                .w(px(4.0))
                .h(px(thumb_height))
                .rounded_full()
                .bg(theme::border_strong())
        })
    }

    fn footer(&self, cx: &App) -> impl IntoElement {
        let model = self.model.read(cx);
        let remaining = model.indexing_remaining();
        div()
            .h(px(30.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px(px(PAGE_PADDING))
            .border_t_1()
            .border_color(theme::border_subtle())
            .font_family(theme::MONO_FONT)
            .text_size(px(11.0))
            .text_color(theme::text_muted())
            .child(format!(
                "{} videos · {}",
                format_count(model.clips.len()),
                format_size(model.total_size_bytes())
            ))
            .children((remaining > 0).then(|| {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().size(px(6.0)).rounded_full().bg(theme::accent()))
                    .child(format!("Indexing {} clips", format_count(remaining)))
            }))
    }
}

impl Render for LibraryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let thumbnail_size = cx.global::<Settings>().thumbnail_size;
        let columns = self.columns(window, thumbnail_size);
        let available = f32::from(window.viewport_size().width) - PAGE_PADDING * 2.0;
        self.card_width = (available - CARD_GAP * (columns as f32 - 1.0)) / columns as f32;
        let revision = self.model.read(cx).revision;
        if self.rows_key != (revision, columns) {
            self.rebuild_rows(columns, cx);
        }
        let empty_message = {
            let model = self.model.read(cx);
            (!model.scanning && model.order.is_empty()).then_some(if model.clips.is_empty() {
                "No videos yet. Add a capture folder in Settings."
            } else {
                "No videos match your search."
            })
        };
        div()
            .key_context("LibraryView")
            .track_focus(&self.focus_handle)
            .on_action(
                cx.listener(|this, _: &FocusSearch, window, cx| this.search.read(cx).focus(window)),
            )
            .size_full()
            .flex()
            .flex_col()
            .child(self.toolbar(window, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .px(px(PAGE_PADDING))
                    .child(
                        list(
                            self.list_state.clone(),
                            cx.processor(|this, index, window, cx| {
                                this.render_row(index, window, cx)
                            }),
                        )
                        .size_full(),
                    )
                    .child(self.scrollbar(window))
                    .children(empty_message.map(|message| {
                        div()
                            .pt(px(48.0))
                            .text_color(theme::text_muted())
                            .text_size(px(14.0))
                            .child(message)
                    })),
            )
            .child(self.footer(cx))
    }
}
