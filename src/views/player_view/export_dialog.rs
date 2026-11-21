use std::path::Path;

use super::*;
use crate::components::icon::icon;
use crate::export_options::scaled_width;
use crate::format::{format_size, format_timecode};
use crate::transcode::{self, Encoder, JobPhase, JobSpec};

impl PlayerView {
    pub(super) fn begin_trim_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(encoder) = self
            .encoders(cx)
            .and_then(|found| found.for_codec(self.export_options.codec))
        else {
            return;
        };
        let options = self.export_options.clone();
        let spec = JobSpec {
            input: self.clip.path.clone(),
            output: PathBuf::new(),
            start_seconds: self.selection.start,
            duration_seconds: self.selection.length(),
            width: scaled_width(self.source, options.height),
            height: options.height,
            frame_rate: options.frame_rate,
            source_height: self.info.height,
            source_frame_rate: self.info.frame_rate,
            encoder,
            codec: options.codec,
            audio: options.audio,
            video_kbps: options.video_kbps,
            audio_kbps: options.audio_kbps,
            target_bytes: None,
        };
        self.choose_output_and_start(spec, "clip", window, cx);
    }

    pub(super) fn begin_compress(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = self.compress_plan();
        let Some(encoder) = self
            .encoders(cx)
            .and_then(|found| found.for_codec(chosen.codec))
        else {
            return;
        };
        let spec = JobSpec {
            input: self.clip.path.clone(),
            output: PathBuf::new(),
            start_seconds: 0.0,
            duration_seconds: self.info.duration_seconds,
            width: chosen.width,
            height: chosen.height,
            frame_rate: chosen.frame_rate,
            source_height: self.info.height,
            source_frame_rate: self.info.frame_rate,
            encoder,
            codec: chosen.codec,
            audio: crate::export_options::AudioChoice::Aac,
            video_kbps: chosen.video_kbps,
            audio_kbps: chosen.audio_kbps,
            target_bytes: Some(self.compress_target.target_bytes()),
        };
        let suffix = format!("{}mb", self.compress_target.megabytes());
        self.choose_output_and_start(spec, &suffix, window, cx);
    }

    fn choose_output_and_start(
        &mut self,
        spec: JobSpec,
        suffix: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let directory = self
            .clip
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let stem = self
            .clip
            .path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let suggested = format!("{stem}-{suffix}.mp4");
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let mut path = path;
            if path.extension().is_none() {
                path.set_extension("mp4");
            }
            this.update(cx, |this, cx| {
                let format_label = format!(
                    "{} · {}p{}",
                    spec.codec.short_label(),
                    spec.height,
                    spec.frame_rate
                );
                let encoder_label = format!(
                    "{} · {}",
                    spec.codec.short_label(),
                    if spec.encoder.hardware { "GPU" } else { "CPU" }
                );
                let mut spec = spec;
                spec.output = path.clone();
                this.job = Some(ActiveJob {
                    handle: transcode::start(spec),
                    output: path,
                    format_label,
                    encoder_label,
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn export_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let Some(job) = &self.job else {
            return div().into_any_element();
        };
        let phase = job.handle.phase();
        let compressing = self.mode == Mode::Compress;
        let output = job.output.to_string_lossy().into_owned();
        let finished = matches!(phase, JobPhase::Done { .. });
        let (title, percent) = match &phase {
            JobPhase::Running { fraction, .. } => (
                if compressing {
                    "Compressing clip"
                } else {
                    "Exporting clip"
                },
                Some(*fraction),
            ),
            JobPhase::Done { .. } => ("Saved", None),
            JobPhase::Failed(_) => ("Export failed", None),
            JobPhase::Cancelled => ("Cancelled", None),
        };
        let bar_color = if finished {
            theme::success()
        } else {
            theme::accent()
        };
        let bar_fraction = match &phase {
            JobPhase::Running { fraction, .. } => *fraction as f32,
            JobPhase::Done { .. } => 1.0,
            _ => 0.0,
        };
        let stats: Vec<(&'static str, String)> = match &phase {
            JobPhase::Running {
                speed,
                elapsed_seconds,
                remaining_seconds,
                ..
            } => vec![
                ("Speed", format!("{speed:.1}×")),
                ("Elapsed", format_timecode(*elapsed_seconds)),
                ("Remaining", format_timecode(*remaining_seconds)),
                ("Encoder", job.encoder_label.clone()),
            ],
            JobPhase::Done {
                bytes,
                took_seconds,
            } => vec![
                ("Size", format_size(*bytes)),
                ("Length", format_timecode(self.exported_length())),
                ("Took", format_timecode(*took_seconds)),
                ("Format", job.format_label.clone()),
            ],
            JobPhase::Failed(message) => vec![("Error", message.clone())],
            JobPhase::Cancelled => Vec::new(),
        };
        let output_for_reveal = job.output.clone();
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x000000b8))
            .occlude()
            .child(
                div()
                    .w(px(680.0))
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .rounded(px(16.0))
                    .bg(gpui::rgb(0x111318))
                    .border_1()
                    .border_color(theme::border())
                    .shadow_lg()
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .when(finished, |this| {
                                        this.child(
                                            div()
                                                .size(px(40.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_full()
                                                .bg(gpui::rgb(0x16281f))
                                                .child(icon("check", 20.0, theme::success())),
                                        )
                                    })
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .text_size(px(17.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(title),
                                            )
                                            .child(
                                                div()
                                                    .font_family(theme::MONO_FONT)
                                                    .text_size(px(11.5))
                                                    .text_color(theme::text_muted())
                                                    .child(SharedString::from(output)),
                                            ),
                                    ),
                            )
                            .children(percent.map(|fraction| {
                                div()
                                    .text_size(px(30.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(SharedString::from(format!("{:.0}%", fraction * 100.0)))
                            })),
                    )
                    .child(
                        div()
                            .h(px(300.0))
                            .w_full()
                            .flex()
                            .rounded(px(10.0))
                            .overflow_hidden()
                            .bg(gpui::black())
                            .children(self.current_image.clone().map(|image| {
                                img(image).size_full().object_fit(ObjectFit::Contain)
                            })),
                    )
                    .child(
                        div()
                            .h(px(6.0))
                            .w_full()
                            .rounded_full()
                            .bg(theme::surface())
                            .child(
                                div()
                                    .h_full()
                                    .w(gpui::relative(bar_fraction))
                                    .rounded_full()
                                    .bg(bar_color),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .children(stats.into_iter().map(|(label, value)| {
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .text_color(theme::text_muted())
                                            .child(label),
                                    )
                                    .child(
                                        div()
                                            .font_family(theme::MONO_FONT)
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(SharedString::from(value)),
                                    )
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_3()
                            .when(matches!(phase, JobPhase::Running { .. }), |this| {
                                this.child(
                                    button("cancel-export", "Cancel", None, ButtonKind::Secondary)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(job) = this.job.take() {
                                                job.handle.cancel();
                                            }
                                            cx.notify();
                                        })),
                                )
                            })
                            .when(!matches!(phase, JobPhase::Running { .. }), |this| {
                                this.child(
                                    button("close-export", "Close", None, ButtonKind::Secondary)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.job = None;
                                            cx.notify();
                                        })),
                                )
                            })
                            .when(finished, |this| {
                                this.child(
                                    button(
                                        "show-in-folder",
                                        "Show in folder",
                                        Some("folder"),
                                        ButtonKind::Primary,
                                    )
                                    .on_click(cx.listener(
                                        move |_, _, _, cx| cx.reveal_path(&output_for_reveal),
                                    )),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }

    fn exported_length(&self) -> f64 {
        match self.mode {
            Mode::Trim => self.selection.length(),
            _ => self.info.duration_seconds,
        }
    }
}

#[allow(dead_code)]
fn _encoder_type_check(_: Encoder) {}
