use gpui::prelude::*;
use gpui::{App, IntoElement, SharedString, Window, div, px};

use crate::components::pointer_surface::{PointerPhase, pointer_surface};
use crate::theme;

const TRACK_HEIGHT: f32 = 4.0;
const THUMB_DIAMETER: f32 = 12.0;
const HIT_HEIGHT: f32 = 20.0;

pub fn slider(
    id: impl Into<SharedString>,
    fraction: f32,
    width: f32,
    window: &mut Window,
    cx: &mut App,
    on_change: impl Fn(f32, &mut App) + 'static,
) -> impl IntoElement {
    let fraction = fraction.clamp(0.0, 1.0);
    pointer_surface(id, window, cx, move |event, cx| {
        if matches!(event.phase, PointerPhase::Down | PointerPhase::Drag) {
            on_change(event.fraction(), cx);
        }
    })
    .w(px(width))
    .h(px(HIT_HEIGHT))
    .flex()
    .items_center()
    .cursor_pointer()
    .child(
        div()
            .relative()
            .w_full()
            .h(px(TRACK_HEIGHT))
            .rounded_full()
            .bg(theme::border_strong())
            .child(
                div()
                    .h_full()
                    .w(gpui::relative(fraction))
                    .rounded_full()
                    .bg(theme::accent()),
            )
            .child(
                div()
                    .absolute()
                    .top(px((TRACK_HEIGHT - THUMB_DIAMETER) / 2.0))
                    .left(gpui::relative(fraction))
                    .ml(px(-THUMB_DIAMETER / 2.0))
                    .size(px(THUMB_DIAMETER))
                    .rounded_full()
                    .bg(theme::accent()),
            ),
    )
}
