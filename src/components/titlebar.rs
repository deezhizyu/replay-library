use gpui::prelude::*;
use gpui::{Div, IntoElement, SharedString, WindowControlArea, div, px};

use crate::components::icon::icon;
use crate::theme;

const HEIGHT: f32 = 44.0;
const BUTTON_WIDTH: f32 = 46.0;

pub fn titlebar(breadcrumb: Option<SharedString>) -> impl IntoElement {
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
                .child(logo())
                .child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_size(px(13.0))
                        .child("Replay Library"),
                )
                .children(breadcrumb.map(|text| {
                    div()
                        .text_size(px(12.0))
                        .text_color(theme::text_muted())
                        .child(SharedString::from(format!("/ {text}")))
                })),
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

pub fn logo() -> Div {
    div()
        .size(px(20.0))
        .flex_none()
        .rounded(px(6.0))
        .bg(theme::accent())
        .flex()
        .items_center()
        .justify_center()
        .child(icon("play", 10.0, theme::on_accent()))
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
