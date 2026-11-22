use gpui::prelude::*;
use gpui::{Div, ElementId, FontWeight, SharedString, Stateful, div, px};

use crate::components::icon::icon;
use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
}

pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon_name: Option<&'static str>,
    kind: ButtonKind,
) -> Stateful<Div> {
    let primary = kind == ButtonKind::Primary;
    let text_color = if primary {
        theme::on_accent()
    } else {
        theme::text()
    };
    div()
        .id(id)
        .h(px(40.0))
        .px_4()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap_2()
        .rounded(px(10.0))
        .text_size(px(13.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(text_color)
        .cursor_pointer()
        .when(primary, |this| {
            this.bg(theme::accent())
                .hover(|style| style.bg(gpui::rgb(0xff8a3a)))
        })
        .when(!primary, |this| {
            this.bg(theme::surface_raised())
                .border_1()
                .border_color(theme::border_strong())
                .hover(|style| style.bg(theme::surface_hover()))
        })
        .children(icon_name.map(|name| icon(name, 16.0, text_color)))
        .child(label.into())
}

pub fn icon_button(id: impl Into<ElementId>, icon_name: &'static str, size: f32) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .hover(|style| style.bg(theme::surface_hover()))
        .child(icon(icon_name, size / 2.0, theme::text()))
}
