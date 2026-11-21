use gpui::prelude::*;
use gpui::{Div, ElementId, Stateful, div, px};

use crate::theme;

pub fn toggle(id: impl Into<ElementId>, enabled: bool) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(40.0))
        .h(px(24.0))
        .flex_none()
        .rounded_full()
        .cursor_pointer()
        .p(px(3.0))
        .flex()
        .items_center()
        .bg(if enabled {
            theme::accent()
        } else {
            theme::border_strong()
        })
        .when(enabled, |this| this.justify_end())
        .child(div().size(px(18.0)).rounded_full().bg(if enabled {
            theme::on_accent()
        } else {
            theme::text_muted()
        }))
}
