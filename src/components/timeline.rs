use gpui::prelude::*;
use gpui::{App, Div, SharedString, Stateful, Window, div, img, px, relative};

use crate::components::pointer_surface::{PointerEvent, pointer_surface};
use crate::media::filmstrip::Filmstrip;
use crate::theme;

pub const TIMELINE_HEIGHT: f32 = 60.0;

pub struct TimelineMarks {
    pub playhead: f32,
    pub hover: Option<f32>,
    pub selection: Option<(f32, f32)>,
}

pub fn timeline(
    id: impl Into<SharedString>,
    filmstrip: &Filmstrip,
    marks: TimelineMarks,
    window: &mut Window,
    cx: &mut App,
    on_pointer: impl Fn(PointerEvent, &mut App) + 'static,
) -> Stateful<Div> {
    let tiles = filmstrip.tiles();
    let dimmed = |left: f32, width: f32| {
        div()
            .absolute()
            .top_0()
            .h_full()
            .left(relative(left))
            .w(relative(width))
            .bg(gpui::rgba(0x0a0b0d99))
    };
    let surface = pointer_surface(id, window, cx, on_pointer)
        .w_full()
        .h(px(TIMELINE_HEIGHT))
        .cursor_pointer();
    let strip = div()
        .absolute()
        .size_full()
        .flex()
        .rounded(px(8.0))
        .overflow_hidden()
        .bg(theme::surface())
        .children(tiles.into_iter().map(|tile| {
            div()
                .h_full()
                .flex_1()
                .overflow_hidden()
                .children(tile.map(|image| {
                    img(image)
                        .w_full()
                        .h_full()
                        .object_fit(gpui::ObjectFit::Cover)
                }))
        }));
    let surface = surface.child(strip);
    let surface = match marks.selection {
        Some((start, end)) => surface
            .child(dimmed(0.0, start))
            .child(dimmed(end, 1.0 - end))
            .child(selection_frame(start, end)),
        None => surface.child(dimmed(marks.playhead, 1.0 - marks.playhead)),
    };
    surface
        .children(marks.hover.map(|fraction| {
            div()
                .absolute()
                .top_0()
                .h_full()
                .left(relative(fraction))
                .w(px(1.0))
                .bg(gpui::rgba(0xffffffcc))
        }))
        .child(
            div()
                .absolute()
                .top(px(-6.0))
                .h(px(TIMELINE_HEIGHT + 12.0))
                .left(relative(marks.playhead))
                .ml(px(-1.0))
                .w(px(2.0))
                .bg(theme::accent())
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(-5.0))
                        .size(px(12.0))
                        .rounded_full()
                        .bg(theme::accent()),
                ),
        )
}

fn selection_frame(start: f32, end: f32) -> Div {
    let handle = |fraction: f32| {
        div()
            .absolute()
            .top(px(-4.0))
            .h(px(TIMELINE_HEIGHT + 8.0))
            .left(relative(fraction))
            .ml(px(-7.0))
            .w(px(14.0))
            .rounded(px(5.0))
            .bg(theme::accent())
            .flex()
            .items_center()
            .justify_center()
            .gap(px(2.0))
            .child(div().w(px(1.5)).h(px(18.0)).bg(theme::on_accent()))
            .child(div().w(px(1.5)).h(px(18.0)).bg(theme::on_accent()))
    };
    div()
        .absolute()
        .top_0()
        .h_full()
        .left(relative(start))
        .w(relative(end - start))
        .border_y_2()
        .border_color(theme::accent())
        .child(div())
        .child(handle(0.0))
        .child(handle(1.0))
}

pub fn preview_left(fraction: f32, timeline_width: f32, preview_width: f32) -> f32 {
    (fraction * timeline_width - preview_width / 2.0)
        .clamp(0.0, (timeline_width - preview_width).max(0.0))
}

pub fn timeline_ruler(marks: Vec<f64>, duration_seconds: f64) -> impl IntoElement {
    div()
        .relative()
        .h(px(18.0))
        .w_full()
        .children(marks.into_iter().map(|mark| {
            let seconds = mark as u64;
            div()
                .absolute()
                .top_0()
                .left(relative((mark / duration_seconds.max(0.001)) as f32))
                .font_family(theme::MONO_FONT)
                .text_size(px(10.5))
                .text_color(theme::text_dim())
                .child(SharedString::from(format!(
                    "{}:{:02}",
                    seconds / 60,
                    seconds % 60
                )))
        }))
}

#[cfg(test)]
mod tests {
    use super::preview_left;

    #[test]
    fn preview_stays_inside_the_timeline() {
        assert_eq!(preview_left(0.0, 1000.0, 192.0), 0.0);
        assert_eq!(preview_left(0.5, 1000.0, 192.0), 404.0);
        assert_eq!(preview_left(1.0, 1000.0, 192.0), 808.0);
        assert_eq!(preview_left(0.5, 100.0, 192.0), 0.0);
    }
}
