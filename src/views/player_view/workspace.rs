use super::*;
use crate::components::timeline::timeline_ruler;
use crate::format::format_timecode;

const PANEL_WIDTH: f32 = 400.0;

impl PlayerView {
    pub(super) fn workspace_layout(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (panel_body, panel_footer) = self.export_panel(cx);
        div()
            .size_full()
            .flex()
            .bg(theme::background())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(self.workspace_header(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .px_8()
                            .pb_6()
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .rounded(px(10.0))
                                    .overflow_hidden()
                                    .bg(gpui::black())
                                    .border_1()
                                    .border_color(theme::border_subtle())
                                    .child(self.video_surface())
                                    .when(self.debug, |this| this.child(self.debug_overlay())),
                            )
                            .child(self.transport_row(cx))
                            .child(self.trim_bottom(window, cx)),
                    ),
            )
            .child(
                div()
                    .w(px(PANEL_WIDTH))
                    .flex_none()
                    .h_full()
                    .flex()
                    .flex_col()
                    .bg(gpui::rgb(0x0d0f12))
                    .border_l_1()
                    .border_color(theme::border_subtle())
                    .p_6()
                    .child(
                        div()
                            .id("side-panel")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(panel_body),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .flex_col()
                            .pt_4()
                            .child(panel_footer),
                    ),
            )
    }

    fn workspace_header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .h(px(64.0))
            .flex_none()
            .px_5()
            .flex()
            .items_center()
            .gap_3()
            .child(
                icon_button("back-to-player", "arrow-left", 40.0)
                    .on_click(cx.listener(|this, _, _, cx| this.enter_mode(Mode::Watch, cx))),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(self.clip.title.clone())),
            )
            .child(
                div()
                    .font_family(theme::MONO_FONT)
                    .text_size(px(12.0))
                    .text_color(theme::text_muted())
                    .child(SharedString::from(self.clip.file_name())),
            )
    }

    fn transport_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let loop_color = if self.loop_clip {
            theme::accent()
        } else {
            theme::text()
        };
        div()
            .h(px(44.0))
            .flex_none()
            .flex()
            .items_center()
            .gap_4()
            .child(
                div()
                    .id("workspace-play")
                    .size(px(44.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(theme::surface_raised())
                    .border_1()
                    .border_color(theme::border_strong())
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::surface_hover()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_playing();
                        cx.notify();
                    }))
                    .child(icon(
                        if self.playing { "pause" } else { "play" },
                        16.0,
                        theme::text(),
                    )),
            )
            .child(self.time_readout())
            .child(div().flex_1())
            .child(
                div()
                    .id("loop-clip")
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(13.0))
                    .text_color(loop_color)
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.loop_clip = !this.loop_clip;
                        cx.notify();
                    }))
                    .child(icon("repeat", 16.0, loop_color))
                    .child("Loop clip"),
            )
    }

    fn trim_bottom(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let chip = |label: &'static str, seconds: f64, highlighted: bool| {
            div()
                .h(px(44.0))
                .px_4()
                .flex()
                .items_center()
                .gap_2()
                .rounded(px(10.0))
                .border_1()
                .when(highlighted, |this| {
                    this.border_color(theme::accent())
                        .bg(theme::accent_soft())
                        .text_color(theme::accent())
                })
                .when(!highlighted, |this| {
                    this.border_color(theme::border()).bg(theme::surface())
                })
                .child(
                    div()
                        .text_size(px(12.0))
                        .when(!highlighted, |this| this.text_color(theme::text_muted()))
                        .child(label),
                )
                .child(
                    div()
                        .font_family(theme::MONO_FONT)
                        .text_size(px(14.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(format_timecode(seconds)),
                )
        };
        let hover = self.hover_fraction;
        let marks = ruler_labels(self.info.duration_seconds);
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(chip("In", self.selection.start, false))
                    .child(chip("Out", self.selection.end, false))
                    .child(chip("Length", self.selection.length(), true))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("reset-selection")
                            .text_size(px(13.0))
                            .text_color(theme::text_muted())
                            .cursor_pointer()
                            .hover(|style| style.text_color(theme::text()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.selection = TrimSelection::whole(this.info.duration_seconds);
                                cx.notify();
                            }))
                            .child("Reset"),
                    ),
            )
            .child(
                div()
                    .relative()
                    .child(self.timeline_element(window, cx))
                    .children(hover.map(|fraction| self.hover_preview(fraction))),
            )
            .child(timeline_ruler(marks, self.info.duration_seconds))
    }
}

fn ruler_labels(duration: f64) -> Vec<f64> {
    crate::trim_selection::ruler_marks(duration)
}
