use super::*;
use crate::components::panel::{choice_card, choice_chip, section_title};
use crate::export_options::{AudioChoice, VideoCodec, available_frame_rates, available_heights};
use crate::format::{format_size, format_timecode};

const CUSTOM_CHIP_CHARACTER_WIDTH: f32 = 8.0;

impl PlayerView {
    pub(super) fn export_panel(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> (impl IntoElement + use<>, impl IntoElement + use<>) {
        let file_size = self.export_tab == ExportTab::FileSize;
        let codec = if file_size {
            VideoCodec::H264
        } else {
            self.export_options.codec
        };
        let encoder_found = self
            .encoders(cx)
            .and_then(|available| available.for_codec(codec))
            .is_some();
        let estimated = self.export_options.estimated_bytes(self.selection.length());
        let tab_chip = |id: &'static str, label: &'static str, tab: ExportTab| {
            choice_chip(id, label, self.export_tab == tab, false, true).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.export_tab = tab;
                    cx.notify();
                },
            ))
        };
        let body = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Export clip"),
                    )
                    .child(
                        div()
                            .font_family(theme::MONO_FONT)
                            .text_size(px(11.5))
                            .text_color(theme::text_muted())
                            .child(SharedString::from(format!(
                                "{:.1} s · {} → {}",
                                self.selection.length(),
                                format_timecode(self.selection.start),
                                format_timecode(self.selection.end)
                            ))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(tab_chip("tab-file-size", "File size", ExportTab::FileSize))
                    .child(tab_chip("tab-custom", "Custom", ExportTab::Custom)),
            )
            .child(if file_size {
                self.file_size_tab(cx).into_any_element()
            } else {
                self.custom_tab(window, cx).into_any_element()
            });
        let footer = div()
            .flex()
            .flex_col()
            .gap_4()
            .when(!file_size, |this| {
                this.child(
                    div()
                        .flex()
                        .items_end()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(theme::text_muted())
                                        .child("Estimated size"),
                                )
                                .child(
                                    div()
                                        .font_family(theme::MONO_FONT)
                                        .text_size(px(11.0))
                                        .text_color(theme::text_dim())
                                        .child(SharedString::from(format!(
                                            "{:.1} Mbps · {} fps",
                                            f64::from(self.export_options.video_kbps) / 1000.0,
                                            self.export_options.frame_rate
                                        ))),
                                ),
                        )
                        .child(
                            div()
                                .font_family(theme::MONO_FONT)
                                .text_size(px(26.0))
                                .font_weight(FontWeight::MEDIUM)
                                .child(SharedString::from(format!("≈ {}", format_size(estimated)))),
                        ),
                )
            })
            .child(
                button(
                    "export-clip",
                    "Export…",
                    Some("download"),
                    ButtonKind::Primary,
                )
                .h(px(52.0))
                .when(!encoder_found, |this| this.opacity(0.5))
                .on_click(cx.listener(|this, _, window, cx| this.begin_export(window, cx))),
            );
        (body, footer)
    }

    fn custom_tab(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let encoders = self.encoders(cx);
        let heights = available_heights(self.source);
        let rates = available_frame_rates(self.source);
        let custom_height = self.custom_height;
        let custom_frame_rate = self.custom_frame_rate;
        let height_digits = self.custom_height_field.read(cx).text().chars().count();
        let video_digits = self.video_bitrate_field.read(cx).text().chars().count();
        let audio_digits = self.audio_bitrate_field.read(cx).text().chars().count();
        let rate_digits = self.custom_frame_rate_field.read(cx).text().chars().count();
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .child(section_title(
                        "Resolution",
                        Some(format!(
                            "source {}×{}",
                            self.source.width, self.source.height
                        )),
                    ))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .children(heights.into_iter().map(|height| {
                                choice_chip(
                                    ("height", height as usize),
                                    format!("{height}p"),
                                    !custom_height && self.export_options.height == height,
                                    true,
                                    true,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.export_options.height = height;
                                        this.custom_height = false;
                                        this.sync_bitrate_fields(cx);
                                        cx.notify();
                                    },
                                ))
                            }))
                            .child(if custom_height {
                                custom_input_chip(
                                    self.custom_height_field.clone(),
                                    height_digits,
                                    "p",
                                    true,
                                )
                                .into_any_element()
                            } else {
                                choice_chip("height-custom", "Custom", false, false, true)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.custom_height = true;
                                        let height = this.export_options.height.to_string();
                                        this.custom_height_field.update(cx, |field, cx| {
                                            field.set_text(height, cx);
                                            field.focus(window);
                                        });
                                        cx.notify();
                                    }))
                                    .into_any_element()
                            }),
                    ),
            )
            .child(
                div()
                    .child(section_title(
                        "Frame rate",
                        Some(format!(
                            "source {} fps",
                            crate::library::nominal_frame_rate(self.source.frame_rate)
                        )),
                    ))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .children(rates.into_iter().map(|rate| {
                                choice_chip(
                                    ("rate", rate as usize),
                                    format!("{rate} fps"),
                                    !custom_frame_rate && self.export_options.frame_rate == rate,
                                    true,
                                    true,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.export_options.frame_rate = rate;
                                        this.custom_frame_rate = false;
                                        this.sync_bitrate_fields(cx);
                                        cx.notify();
                                    },
                                ))
                            }))
                            .child(if custom_frame_rate {
                                custom_input_chip(
                                    self.custom_frame_rate_field.clone(),
                                    rate_digits,
                                    " fps",
                                    true,
                                )
                                .into_any_element()
                            } else {
                                choice_chip("rate-custom", "Custom", false, false, true)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.custom_frame_rate = true;
                                        let rate = this.export_options.frame_rate.to_string();
                                        this.custom_frame_rate_field.update(cx, |field, cx| {
                                            field.set_text(rate, cx);
                                            field.focus(window);
                                        });
                                        cx.notify();
                                    }))
                                    .into_any_element()
                            }),
                    ),
            )
            .child(
                div()
                    .child(section_title(
                        "Video codec",
                        VideoCodec::from_label(&self.info.codec)
                            .map(|codec| format!("source {}", codec.short_label())),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .children(VideoCodec::ALL.map(|codec| {
                                let available =
                                    encoders.is_none_or(|found| found.for_codec(codec).is_some());
                                let note =
                                    (!available).then(|| SharedString::from("No encoder found"));
                                choice_card(
                                    ("codec", codec as usize),
                                    codec.label(),
                                    codec.tagline(),
                                    note,
                                    self.export_options.codec == codec,
                                    available,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if available {
                                            this.export_options.codec = codec;
                                            this.export_options.video_kbps_edited = false;
                                            this.sync_bitrate_fields(cx);
                                            cx.notify();
                                        }
                                    },
                                ))
                            })),
                    ),
            )
            .child(
                div()
                    .child(section_title(
                        "Audio codec",
                        AudioChoice::from_label(&self.info.audio_codec)
                            .map(|audio| format!("source {}", audio.label())),
                    ))
                    .child(div().flex().gap_2().children(AudioChoice::ALL.map(|audio| {
                        choice_chip(
                            ("audio", audio as usize),
                            audio.label(),
                            self.export_options.audio == audio,
                            false,
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.export_options.select_audio(audio);
                            this.sync_bitrate_fields(cx);
                            cx.notify();
                        }))
                    }))),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(bitrate_input(
                        "Video bitrate",
                        format!("source {:.1}", f64::from(self.streams.video_kbps) / 1000.0),
                        self.video_bitrate_field.clone(),
                        " Mbps",
                        video_digits,
                        self.video_bitrate_field.read(cx).is_focused(window),
                    ))
                    .child(bitrate_input(
                        "Audio bitrate",
                        format!("source {}", self.streams.audio_kbps),
                        self.audio_bitrate_field.clone(),
                        " kbps",
                        audio_digits,
                        self.audio_bitrate_field.read(cx).is_focused(window),
                    )),
            )
    }
}

fn bitrate_input(
    label: &'static str,
    source: String,
    field: Entity<TextField>,
    suffix: &'static str,
    digits: usize,
    active: bool,
) -> impl IntoElement {
    div()
        .flex_1()
        .min_w_0()
        .child(section_title(label, Some(source)))
        .child(custom_input_chip(field, digits, suffix, active))
}

fn custom_input_chip(
    field: Entity<TextField>,
    digits: usize,
    suffix: &'static str,
    active: bool,
) -> impl IntoElement {
    div()
        .h(px(44.0))
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.0))
        .border_1()
        .when(active, |this| {
            this.border_color(theme::accent())
                .bg(theme::accent_soft())
                .text_color(theme::accent())
        })
        .when(!active, |this| {
            this.border_color(theme::border())
                .bg(theme::surface())
                .text_color(theme::text())
        })
        .font_family(theme::MONO_FONT)
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
        .child(
            div()
                .w(px(CUSTOM_CHIP_CHARACTER_WIDTH * digits.max(1) as f32 + 4.0))
                .h(px(20.0))
                .child(field),
        )
        .child(suffix)
}
