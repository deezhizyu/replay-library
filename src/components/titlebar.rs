use gpui::prelude::*;
use gpui::{IntoElement, WindowControlArea, div, px};

use crate::components::icon::icon;
use crate::theme;

const HEIGHT: f32 = 34.0;
const BUTTON_WIDTH: f32 = 46.0;

pub fn titlebar() -> impl IntoElement {
    div()
        .h(px(HEIGHT))
        .flex_none()
        .flex()
        .items_center()
        .justify_between()
        .pl_4()
        .bg(theme::background())
        .border_b_1()
        .border_color(theme::border_subtle())
        .child(
            div()
                .window_control_area(WindowControlArea::Drag)
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .gap_2p5()
                .child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_size(px(13.0))
                        .child("Replay Library"),
                ),
        )
        .child(
            div()
                .flex()
                .h_full()
                .child(window_button("minimize", WindowControlArea::Min, false))
                .child(window_button("maximize", WindowControlArea::Max, false))
                .child(window_button("close", WindowControlArea::Close, true)),
        )
}

fn window_button(
    name: &'static str,
    area: WindowControlArea,
    destructive: bool,
) -> impl IntoElement {
    div()
        .window_control_area(area)
        .w(px(BUTTON_WIDTH))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .hover(move |style| {
            if destructive {
                style.bg(theme::close_hover())
            } else {
                style.bg(theme::surface_raised())
            }
        })
        .child(icon(name, 16.0, theme::text_muted()))
}
