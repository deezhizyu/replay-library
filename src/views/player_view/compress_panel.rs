use super::*;
use crate::components::panel::{section_title, table_row};
use crate::compress_plan::{CompressPlan, plan};
use crate::format::format_size;
use crate::views::player_view::trim_panel::encoder_row;

impl PlayerView {
    pub(super) fn compress_plan(&self) -> CompressPlan {
        plan(
            self.compress_target,
            self.info.duration_seconds,
            self.source,
        )
    }

    pub(super) fn compress_panel(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let chosen = self.compress_plan();
        let encoder = self
            .encoders(cx)
            .and_then(|available| available.for_codec(chosen.codec));
        let result = chosen.estimated_bytes(self.info.duration_seconds);
        let target = self.compress_target;
        let preset = |id: &'static str,
                      megabytes: u32,
                      tagline: &'static str,
                      chosen_target: CompressTarget,
                      detail: String| {
            let selected = target == chosen_target;
            div()
                .id(id)
                .h(px(76.0))
                .w_full()
                .px_4()
                .flex()
                .items_center()
                .gap_4()
                .rounded(px(12.0))
                .border_1()
                .cursor_pointer()
                .when(selected, |this| {
                    this.border_color(theme::accent()).bg(theme::accent_soft())
                })
                .when(!selected, |this| {
                    this.border_color(theme::border()).bg(theme::surface())
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.compress_target = chosen_target;
                    cx.notify();
                }))
                .child(radio_dot(selected))
                .child(
                    div()
                        .w(px(84.0))
                        .font_family(theme::MONO_FONT)
                        .text_size(px(24.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(format!("{megabytes} MB"))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(13.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(tagline),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(theme::text_muted())
                                .child(SharedString::from(detail)),
                        ),
                )
        };
        let preview_detail = |chosen_target: CompressTarget| {
            let preview = plan(chosen_target, self.info.duration_seconds, self.source);
            format!(
                "{}p · {} fps · {}",
                preview.height,
                preview.frame_rate,
                preview.codec.short_label()
            )
        };
        let custom_selected = matches!(target, CompressTarget::Custom(_));
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_size(px(18.0)).font_weight(FontWeight::SEMIBOLD).child("Compress to size"))
                    .child(div().text_size(px(12.5)).text_color(theme::text_muted()).child("Pick a target. The app chooses resolution, frame rate and bitrate to fit.")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(preset("target-8", 8, "Chat & email", CompressTarget::Megabytes8, preview_detail(CompressTarget::Megabytes8)))
                    .child(preset("target-20", 20, "Higher quality", CompressTarget::Megabytes20, preview_detail(CompressTarget::Megabytes20)))
                    .child(
                        div()
                            .id("target-custom")
                            .h(px(76.0))
                            .w_full()
                            .px_4()
                            .flex()
                            .items_center()
                            .gap_4()
                            .rounded(px(12.0))
                            .border_1()
                            .cursor_pointer()
                            .when(custom_selected, |this| this.border_color(theme::accent()).bg(theme::accent_soft()))
                            .when(!custom_selected, |this| this.border_color(theme::border()).bg(theme::surface()))
                            .on_click(cx.listener(|this, _, window, cx| {
                                let megabytes = this.custom_megabytes.read(cx).text().parse::<u32>().unwrap_or(12).clamp(1, 10_000);
                                this.compress_target = CompressTarget::Custom(megabytes);
                                this.custom_megabytes.update(cx, |field, cx| {
                                    field.set_text(megabytes.to_string(), cx);
                                    field.focus(window);
                                });
                                cx.notify();
                            }))
                            .child(radio_dot(custom_selected))
                            .child(
                                div()
                                    .w(px(84.0))
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .font_family(theme::MONO_FONT)
                                    .text_size(px(24.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(div().w(px(48.0)).h(px(32.0)).child(self.custom_megabytes.clone()))
                                    .child(div().text_size(px(14.0)).text_color(theme::text_muted()).child("MB")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(div().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child("Custom"))
                                    .child(div().text_size(px(11.5)).text_color(theme::text_muted()).child("Type any target size")),
                            ),
                    ),
            )
            .child(div().child(section_title("What you get", None)).child(
                div()
                    .flex()
                    .flex_col()
                    .child(table_row("Resolution", format!("{}×{}", chosen.width, chosen.height)))
                    .child(table_row("Frame rate", format!("{} fps", chosen.frame_rate)))
                    .child(table_row("Codec", chosen.codec.short_label().to_string()))
                    .child(table_row("Video bitrate", format!("{:.1} Mbps", f64::from(chosen.video_kbps) / 1000.0)))
                    .child(table_row("Audio bitrate", format!("{} kbps", chosen.audio_kbps)))
                    .child(table_row("Result", format_size(result))),
            ))
            .child(encoder_row(encoder.map(|found| (found.name, found.hardware))))
            .child(
                button("compress-and-save", "Compress & save…", Some("zap"), ButtonKind::Primary)
                    .w_full()
                    .h(px(52.0))
                    .when(encoder.is_none(), |this| this.opacity(0.5))
                    .on_click(cx.listener(|this, _, window, cx| this.begin_compress(window, cx))),
            )
            .child(
                div()
                    .id("switch-to-trim")
                    .flex()
                    .justify_center()
                    .text_size(px(13.0))
                    .text_color(theme::text_muted())
                    .cursor_pointer()
                    .hover(|style| style.text_color(theme::text()))
                    .on_click(cx.listener(|this, _, _, cx| this.enter_mode(Mode::Trim, cx)))
                    .child("Trim & pick resolution instead"),
            )
    }
}

fn radio_dot(selected: bool) -> impl IntoElement {
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
        })
}
