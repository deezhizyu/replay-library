use gpui::prelude::*;
use gpui::{Div, ElementId, FontWeight, IntoElement, SharedString, Stateful, div, px};

use crate::theme;

pub fn section_title(label: &str, detail: Option<String>) -> Div {
    div()
        .flex()
        .items_baseline()
        .gap_2()
        .mb_2()
        .child(
            div()
                .font_family(theme::MONO_FONT)
                .text_size(px(10.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::text_muted())
                .child(SharedString::from(label.to_uppercase())),
        )
        .children(detail.map(|text| {
            div()
                .text_size(px(11.0))
                .text_color(theme::text_dim())
                .child(SharedString::from(text))
        }))
}

pub fn choice_chip(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    mono: bool,
    enabled: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(44.0))
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.0))
        .border_1()
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
        .when(mono, |this| this.font_family(theme::MONO_FONT))
        .when(enabled, |this| this.cursor_pointer())
        .when(!enabled, |this| this.opacity(0.4))
        .when(selected, |this| {
            this.border_color(theme::accent())
                .bg(theme::accent_soft())
                .text_color(theme::accent())
        })
        .when(!selected, |this| {
            this.border_color(theme::border())
                .bg(theme::surface())
                .text_color(theme::text())
                .when(enabled, |this| {
                    this.hover(|style| style.bg(theme::surface_hover()))
                })
        })
        .child(label.into())
}

pub fn choice_card(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    subtitle: impl Into<SharedString>,
    note: Option<SharedString>,
    selected: bool,
    enabled: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(52.0))
        .w_full()
        .px_4()
        .flex()
        .items_center()
        .gap_3()
        .rounded(px(10.0))
        .border_1()
        .when(enabled, |this| this.cursor_pointer())
        .when(!enabled, |this| this.opacity(0.4))
        .when(selected, |this| {
            this.border_color(theme::accent()).bg(theme::accent_soft())
        })
        .when(!selected, |this| {
            this.border_color(theme::border()).bg(theme::surface())
        })
        .child(
            div()
                .size(px(18.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_2()
                .border_color(if selected {
                    theme::accent()
                } else {
                    theme::border_strong()
                })
                .when(selected, |this| {
                    this.child(div().size(px(8.0)).rounded_full().bg(theme::accent()))
                }),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.into()),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(theme::text_muted())
                        .child(subtitle.into()),
                ),
        )
        .children(note.map(|text| {
            div()
                .text_size(px(11.0))
                .text_color(theme::text_dim())
                .child(text)
        }))
}

pub fn table_row(label: &str, value: String) -> impl IntoElement {
    div()
        .h(px(40.0))
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(theme::border_subtle())
        .text_size(px(13.0))
        .child(
            div()
                .text_color(theme::text_muted())
                .child(SharedString::from(label.to_string())),
        )
        .child(
            div()
                .font_family(theme::MONO_FONT)
                .font_weight(FontWeight::MEDIUM)
                .child(SharedString::from(value)),
        )
}
