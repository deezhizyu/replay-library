use super::*;
use crate::components::panel::{choice_card, choice_chip, section_title};
use crate::export_options::{AudioChoice, VideoCodec, available_frame_rates, available_heights};
use crate::format::{format_size, format_timecode};

impl PlayerView {
    pub(super) fn trim_panel(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let encoders = self.encoders(cx);
        let encoder = encoders.and_then(|available| available.for_codec(self.export_options.codec));
        let estimated = self.export_options.estimated_bytes(self.selection.length());
        let heights = available_heights(self.source);
        let rates = available_frame_rates(self.source);
        div()
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
                                    self.export_options.height == height,
                                    true,
                                    true,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.export_options.height = height;
                                        this.sync_bitrate_fields(cx);
                                        cx.notify();
                                    },
                                ))
                            })),
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
                    .child(div().flex().gap_2().children(rates.into_iter().map(|rate| {
                        choice_chip(
                            ("rate", rate as usize),
                            format!("{rate} fps"),
                            self.export_options.frame_rate == rate,
                            true,
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.export_options.frame_rate = rate;
                            this.sync_bitrate_fields(cx);
                            cx.notify();
                        }))
                    }))),
            )
            .child(
                div().child(section_title("Video codec", None)).child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(VideoCodec::ALL.map(|codec| {
                            let available =
                                encoders.is_none_or(|found| found.for_codec(codec).is_some());
                            let note = (!available).then(|| SharedString::from("No encoder found"));
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
                                        this.sync_bitrate_fields(cx);
                                        cx.notify();
                                    }
                                },
                            ))
                        })),
                ),
            )
            .child(div().child(section_title("Audio codec", None)).child(
                div().flex().gap_2().children(AudioChoice::ALL.map(|audio| {
                    choice_chip(
                        ("audio", audio as usize),
                        audio.label(),
                        self.export_options.audio == audio,
                        false,
                        true,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.export_options.audio = audio;
                        cx.notify();
                    }))
                })),
            ))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(bitrate_tile(
                        "Video bitrate",
                        "Mbps",
                        self.video_bitrate_field.clone(),
                    ))
                    .child(bitrate_tile(
                        "Audio bitrate",
                        "kbps",
                        self.audio_bitrate_field.clone(),
                    )),
            )
            .child(encoder_row(
                encoder.map(|found| (found.name, found.hardware)),
            ))
            .child(
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
            .child(
                button(
                    "export-clip",
                    "Export…",
                    Some("download"),
                    ButtonKind::Primary,
                )
                .w_full()
                .h(px(52.0))
                .when(encoder.is_none(), |this| this.opacity(0.5))
                .on_click(cx.listener(|this, _, window, cx| this.begin_trim_export(window, cx))),
            )
            .child(
                div()
                    .id("switch-to-compress")
                    .flex()
                    .justify_center()
                    .text_size(px(13.0))
                    .text_color(theme::text_muted())
                    .cursor_pointer()
                    .hover(|style| style.text_color(theme::text()))
                    .on_click(cx.listener(|this, _, _, cx| this.enter_mode(Mode::Compress, cx)))
                    .child("Compress to 8 / 20 MB instead"),
            )
    }
}

fn bitrate_tile(
    label: &'static str,
    unit: &'static str,
    field: Entity<TextField>,
) -> impl IntoElement {
    div()
        .flex_1()
        .h(px(60.0))
        .px_3()
        .flex()
        .flex_col()
        .justify_center()
        .rounded(px(10.0))
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface())
        .child(
            div()
                .text_size(px(11.0))
                .text_color(theme::text_muted())
                .child(label),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .font_family(theme::MONO_FONT)
                .font_weight(FontWeight::MEDIUM)
                .child(div().flex_1().h(px(22.0)).child(field))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme::text_dim())
                        .child(unit),
                ),
        )
}

pub(super) fn encoder_row(encoder: Option<(&'static str, bool)>) -> impl IntoElement {
    let (label, badge) = match encoder {
        Some((name, true)) => (format!("GPU encoder · {name}"), Some("Hardware")),
        Some((name, false)) => (format!("Software encoder · {name}"), Some("Software")),
        None => ("Detecting encoders…".to_string(), None),
    };
    div()
        .h(px(48.0))
        .px_3()
        .flex()
        .items_center()
        .gap_3()
        .rounded(px(10.0))
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface())
        .child(icon("cpu", 16.0, theme::text_muted()))
        .child(
            div()
                .flex_1()
                .text_size(px(12.5))
                .child(SharedString::from(label)),
        )
        .children(badge.map(|text| {
            div()
                .h(px(22.0))
                .px_2()
                .flex()
                .items_center()
                .rounded(px(6.0))
                .bg(gpui::rgb(0x16233b))
                .text_color(gpui::rgb(0x6fa8ff))
                .text_size(px(11.0))
                .font_weight(FontWeight::MEDIUM)
                .child(text)
        }))
}
