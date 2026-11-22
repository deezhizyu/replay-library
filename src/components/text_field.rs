use gpui::prelude::*;
use gpui::{
    Context, EventEmitter, FocusHandle, Focusable, IntoElement, KeyDownEvent, MouseButton, Render,
    SharedString, Window, div, px,
};

use crate::theme;

pub struct TextChanged(pub String);

pub struct TextField {
    text: String,
    placeholder: SharedString,
    numeric_only: bool,
    maximum_length: Option<usize>,
    inherit_text_size: bool,
    centered: bool,
    focus_handle: FocusHandle,
}

impl EventEmitter<TextChanged> for TextField {}

impl Focusable for TextField {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TextField {
    pub fn new(
        placeholder: impl Into<SharedString>,
        numeric_only: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            text: String::new(),
            placeholder: placeholder.into(),
            numeric_only,
            maximum_length: None,
            inherit_text_size: false,
            centered: false,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn with_maximum_length(mut self, maximum_length: usize) -> Self {
        self.maximum_length = Some(maximum_length);
        self
    }

    pub fn inheriting_text_size(mut self) -> Self {
        self.inherit_text_size = true;
        self
    }

    pub fn centered(mut self) -> Self {
        self.centered = true;
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.text = text.into();
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window) {
        window.focus(&self.focus_handle);
    }

    fn handle_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let command = keystroke.modifiers.control || keystroke.modifiers.platform;
        match keystroke.key.as_str() {
            "backspace" => {
                self.text.pop();
            }
            "enter" => return,
            "v" if command => {
                let pasted = cx
                    .read_from_clipboard()
                    .and_then(|item| item.text())
                    .unwrap_or_default();
                self.insert(&pasted);
            }
            _ if command => return,
            _ => match &keystroke.key_char {
                Some(typed) => self.insert(typed),
                None => return,
            },
        }
        cx.emit(TextChanged(self.text.clone()));
        cx.notify();
    }

    fn insert(&mut self, typed: &str) {
        let numeric_only = self.numeric_only;
        let remaining = self.maximum_length.map_or(usize::MAX, |maximum| {
            maximum.saturating_sub(self.text.chars().count())
        });
        self.text.extend(
            typed
                .chars()
                .filter(|character| {
                    !character.is_control()
                        && (!numeric_only || character.is_ascii_digit() || *character == '.')
                })
                .take(remaining),
        );
    }
}

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        let showing_placeholder = self.text.is_empty();
        div()
            .id("text-field")
            .track_focus(&self.focus_handle)
            .on_key_down(
                cx.listener(|this, event: &KeyDownEvent, _, cx| this.handle_key(event, cx)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, _| window.focus(&this.focus_handle)),
            )
            .flex()
            .items_center()
            .h_full()
            .w_full()
            .overflow_hidden()
            .when(!self.inherit_text_size, |this| this.text_size(px(14.0)))
            .when(self.centered, |this| this.justify_center())
            .child(
                div()
                    .when(showing_placeholder, |this| {
                        this.text_color(theme::text_dim())
                    })
                    .child(if showing_placeholder {
                        self.placeholder.clone()
                    } else {
                        SharedString::from(self.text.clone())
                    }),
            )
            .when(focused, |this| {
                this.child(
                    div()
                        .w(px(1.0))
                        .when(self.inherit_text_size, |caret| caret.h(gpui::relative(0.7)))
                        .when(!self.inherit_text_size, |caret| caret.h(px(16.0)))
                        .ml(px(1.0))
                        .bg(theme::accent()),
                )
            })
    }
}
