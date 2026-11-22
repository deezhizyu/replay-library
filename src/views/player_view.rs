mod compress_panel;
mod export_dialog;
mod trim_panel;
mod workspace;

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, IntoElement,
    KeyDownEvent, ObjectFit, Pixels, Render, RenderImage, SharedString, Window, canvas, div, img,
    px,
};

use crate::components::button::{ButtonKind, button, icon_button};
use crate::components::icon::icon;
use crate::components::pointer_surface::PointerPhase;
use crate::components::slider::slider;
use crate::components::text_field::{TextChanged, TextField};
use crate::components::timeline::{TIMELINE_HEIGHT, TimelineMarks, preview_left, timeline};
use crate::compress_plan::CompressTarget;
use crate::export_options::{ExportOptions, SourceFormat};
use crate::format::format_timecode;
use crate::library::{Clip, ClipInfo, taken_label};
use crate::local_time::{Date, LocalDateTime};
use crate::media::audio::AudioPlayer;
use crate::media::filmstrip::{Filmstrip, PLAYER_TILE_COUNT};
use crate::media::playback::Playback;
use crate::theme;
use crate::transcode::{EncoderCatalog, Encoders, JobHandle};
use crate::trim_selection::TrimSelection;

const HOVER_PREVIEW_WIDTH: f32 = 192.0;
const SPEEDS: [f64; 5] = [0.25, 0.5, 1.0, 1.5, 2.0];
const SEEK_STEP_SECONDS: f64 = 5.0;
const TARGET_RESIZE_THRESHOLD: f32 = 16.0;
const HANDLE_GRAB_PIXELS: f32 = 14.0;
const END_EPSILON_SECONDS: f64 = 0.01;
const AUDIO_DRIFT_TOLERANCE_SECONDS: f64 = 0.02;
const AUDIO_DRIFT_LIMIT_SECONDS: f64 = 0.5;
const AUDIO_DRIFT_CORRECTION: f64 = 0.25;

pub enum PlayerEvent {
    Back,
    Delete,
    Navigate(isize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Watch,
    Trim,
    Compress,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Handle {
    Start,
    End,
}

struct ActiveJob {
    handle: JobHandle,
    output: PathBuf,
    format_label: String,
    encoder_label: String,
}

pub struct PlayerView {
    clip: Clip,
    info: ClipInfo,
    source: SourceFormat,
    playback: Playback,
    audio: AudioPlayer,
    filmstrip: Filmstrip,
    current_image: Option<Arc<RenderImage>>,
    mode: Mode,
    playing: bool,
    base_position: f64,
    base_instant: Instant,
    speed_index: usize,
    volume: f32,
    muted: bool,
    hover_fraction: Option<f32>,
    timeline_width: f32,
    scrubbing: bool,
    resume_after_scrub: bool,
    awaiting_frame: bool,
    video_size: Rc<Cell<(f32, f32)>>,
    sent_target: (f32, f32),
    subtitle: SharedString,
    focus_handle: FocusHandle,
    debug: bool,
    presented_frames: u32,
    presented_window_start: Instant,
    presented_fps: f32,
    seek_started: Option<Instant>,
    seek_latency_milliseconds: f32,
    selection: TrimSelection,
    dragged_handle: Option<Handle>,
    loop_clip: bool,
    export_options: ExportOptions,
    compress_target: CompressTarget,
    custom_megabytes: Entity<TextField>,
    video_bitrate_field: Entity<TextField>,
    audio_bitrate_field: Entity<TextField>,
    job: Option<ActiveJob>,
}

impl EventEmitter<PlayerEvent> for PlayerView {}

impl Focusable for PlayerView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl PlayerView {
    pub fn new(clip: Clip, info: ClipInfo, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let target = initial_target(window);
        let playback = Playback::open(clip.path.clone(), info.clone(), target);
        let clip_path = clip.path.clone();
        let filmstrip =
            Filmstrip::start(clip.path.clone(), info.duration_seconds, PLAYER_TILE_COUNT);
        let today = LocalDateTime::now().map_or(
            Date {
                year: 1970,
                month: 1,
                day: 1,
            },
            |now| now.date,
        );
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);
        let source = SourceFormat {
            width: info.width,
            height: info.height,
            frame_rate: info.frame_rate,
        };
        let custom_megabytes = cx.new(|cx| TextField::new("12", true, cx));
        let video_bitrate_field = cx.new(|cx| TextField::new("Mbps", true, cx));
        let audio_bitrate_field = cx.new(|cx| TextField::new("kbps", true, cx));
        cx.subscribe(
            &custom_megabytes,
            |this, _, TextChanged(text): &TextChanged, cx| {
                if let Ok(megabytes) = text.parse::<u32>() {
                    this.compress_target = CompressTarget::Custom(megabytes.clamp(1, 10_000));
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe(
            &video_bitrate_field,
            |this, _, TextChanged(text): &TextChanged, cx| {
                if let Ok(megabits) = text.parse::<f32>() {
                    this.export_options.video_kbps = ((megabits * 1000.0) as u32).max(100);
                    this.export_options.video_kbps_edited = true;
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe(
            &audio_bitrate_field,
            |this, _, TextChanged(text): &TextChanged, cx| {
                if let Ok(kbps) = text.parse::<u32>() {
                    this.export_options.audio_kbps = kbps.clamp(32, 512);
                    cx.notify();
                }
            },
        )
        .detach();
        let export_options = ExportOptions::for_source(source);
        let mut view = Self {
            subtitle: taken_label(clip.taken, today).into(),
            selection: TrimSelection::whole(info.duration_seconds),
            clip,
            info,
            source,
            playback,
            audio: AudioPlayer::open(clip_path),
            filmstrip,
            current_image: None,
            mode: Mode::Watch,
            playing: false,
            base_position: 0.0,
            base_instant: Instant::now(),
            speed_index: 2,
            volume: 0.8,
            muted: false,
            hover_fraction: None,
            timeline_width: 0.0,
            scrubbing: false,
            resume_after_scrub: false,
            awaiting_frame: true,
            video_size: Rc::new(Cell::new((0.0, 0.0))),
            sent_target: (target.0 as f32, target.1 as f32),
            focus_handle,
            debug: false,
            presented_frames: 0,
            presented_window_start: Instant::now(),
            presented_fps: 0.0,
            seek_started: None,
            seek_latency_milliseconds: 0.0,
            dragged_handle: None,
            loop_clip: false,
            export_options,
            compress_target: CompressTarget::Megabytes8,
            custom_megabytes,
            video_bitrate_field,
            audio_bitrate_field,
            job: None,
        };
        view.sync_bitrate_fields(cx);
        view.set_playing(true);
        view
    }

    pub fn clip(&self) -> &Clip {
        &self.clip
    }

    pub fn position(&self) -> f64 {
        let position = if self.playing {
            self.base_position
                + self.base_instant.elapsed().as_secs_f64() * SPEEDS[self.speed_index]
        } else {
            self.base_position
        };
        position.clamp(0.0, self.info.duration_seconds)
    }

    fn encoders(&self, cx: &App) -> Option<Encoders> {
        cx.global::<EncoderCatalog>().encoders()
    }

    fn playback_bounds(&self) -> (f64, f64) {
        if self.mode == Mode::Trim {
            (self.selection.start, self.selection.end)
        } else {
            (0.0, self.info.duration_seconds)
        }
    }

    fn set_playing(&mut self, playing: bool) {
        if playing == self.playing {
            return;
        }
        self.base_position = self.position();
        self.base_instant = Instant::now();
        let (start, end) = self.playback_bounds();
        if playing && (self.base_position >= end - 0.05 || self.base_position < start) {
            self.seek_to(start, true);
        }
        self.playing = playing;
        self.playback.set_playing(playing);
        self.audio.set_playing(playing);
    }

    fn toggle_playing(&mut self) {
        self.set_playing(!self.playing);
    }

    fn seek_to(&mut self, seconds: f64, exact: bool) {
        let seconds = seconds.clamp(0.0, self.info.duration_seconds);
        self.base_position = seconds;
        self.base_instant = Instant::now();
        self.awaiting_frame = true;
        self.seek_started.get_or_insert_with(Instant::now);
        self.playback.seek(seconds, exact);
        self.audio.seek(seconds);
    }

    fn seek_relative(&mut self, delta: f64) {
        self.seek_to(self.position() + delta, true);
    }

    fn step_frame(&mut self, direction: f64) {
        self.set_playing(false);
        let frame_duration = 1.0 / f64::from(self.info.frame_rate.max(1.0));
        if direction > 0.0 {
            self.base_position =
                (self.base_position + frame_duration).min(self.info.duration_seconds);
            self.awaiting_frame = true;
            self.playback.step_frame();
        } else {
            self.seek_to(self.base_position - frame_duration, true);
        }
    }

    fn cycle_speed(&mut self) {
        self.base_position = self.position();
        self.base_instant = Instant::now();
        self.speed_index = (self.speed_index + 1) % SPEEDS.len();
        self.audio.set_enabled(SPEEDS[self.speed_index] == 1.0);
    }

    fn pointer_on_timeline(&mut self, phase: PointerPhase, fraction: f32, width: f32) {
        let seconds = f64::from(fraction) * self.info.duration_seconds;
        self.timeline_width = width;
        match phase {
            PointerPhase::Hover => self.hover_fraction = Some(fraction),
            PointerPhase::Leave => self.hover_fraction = None,
            PointerPhase::Down => {
                self.resume_after_scrub = self.playing;
                self.set_playing(false);
                self.scrubbing = true;
                self.hover_fraction = Some(fraction);
                self.dragged_handle = (self.mode == Mode::Trim)
                    .then(|| self.handle_near(fraction, width))
                    .flatten();
                self.drag_to(seconds, false);
            }
            PointerPhase::Drag => {
                self.hover_fraction = Some(fraction);
                self.drag_to(seconds, false);
            }
            PointerPhase::Up => {
                self.scrubbing = false;
                self.drag_to(seconds, true);
                self.dragged_handle = None;
                if self.resume_after_scrub {
                    self.set_playing(true);
                }
            }
        }
    }

    fn handle_near(&self, fraction: f32, width: f32) -> Option<Handle> {
        let (start, end) = self.selection.fractions();
        let grab = HANDLE_GRAB_PIXELS / width.max(1.0);
        let start_distance = (fraction - start).abs();
        let end_distance = (fraction - end).abs();
        match (start_distance <= grab, end_distance <= grab) {
            (true, true) => Some(if start_distance <= end_distance {
                Handle::Start
            } else {
                Handle::End
            }),
            (true, false) => Some(Handle::Start),
            (false, true) => Some(Handle::End),
            (false, false) => None,
        }
    }

    fn drag_to(&mut self, seconds: f64, exact: bool) {
        match self.dragged_handle {
            Some(Handle::Start) => {
                self.selection.set_start(seconds);
                self.seek_to(self.selection.start, exact);
            }
            Some(Handle::End) => {
                self.selection.set_end(seconds);
                self.seek_to(self.selection.end, exact);
            }
            None => self.seek_to(seconds, exact),
        }
    }

    fn enter_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.set_playing(false);
        self.mode = mode;
        self.job = None;
        self.sync_bitrate_fields(cx);
        cx.notify();
    }

    fn sync_bitrate_fields(&mut self, cx: &mut Context<Self>) {
        self.export_options.recompute_bitrate(self.source);
        let megabits = format!("{:.1}", f64::from(self.export_options.video_kbps) / 1000.0);
        let audio = self.export_options.audio_kbps.to_string();
        self.video_bitrate_field
            .update(cx, |field, cx| field.set_text(megabits, cx));
        self.audio_bitrate_field
            .update(cx, |field, cx| field.set_text(audio, cx));
    }

    fn follow_audio_clock(&mut self) {
        self.audio
            .set_volume(if self.muted { 0.0 } else { self.volume });
        if !self.playing || SPEEDS[self.speed_index] != 1.0 {
            return;
        }
        if let Some(audio_position) = self.audio.position() {
            let drift = audio_position - self.position();
            if drift.abs() > AUDIO_DRIFT_TOLERANCE_SECONDS
                && drift.abs() < AUDIO_DRIFT_LIMIT_SECONDS
            {
                self.base_position += drift * AUDIO_DRIFT_CORRECTION;
            }
        }
    }

    fn advance(&mut self, window: &mut Window) {
        self.follow_audio_clock();
        let position = self.position();
        if let Some(image) = self.playback.take_frame(position) {
            if let Some(previous) = self.current_image.replace(image) {
                window.drop_image(previous).ok();
            }
            self.awaiting_frame = false;
            self.presented_frames += 1;
            if let Some(started) = self.seek_started.take() {
                self.seek_latency_milliseconds = started.elapsed().as_secs_f32() * 1000.0;
            }
        }
        let window_seconds = self.presented_window_start.elapsed().as_secs_f32();
        if window_seconds >= 1.0 {
            self.presented_fps = self.presented_frames as f32 / window_seconds;
            self.presented_frames = 0;
            self.presented_window_start = Instant::now();
        }
        let (start, end) = self.playback_bounds();
        let at_end = position >= end - END_EPSILON_SECONDS || self.playback.finished();
        if self.playing && at_end {
            if self.mode == Mode::Trim && self.loop_clip {
                self.seek_to(start, true);
            } else {
                self.base_position = end;
                self.playing = false;
                self.playback.set_playing(false);
                self.audio.set_playing(false);
            }
        }
        let (width, height) = self.video_size.get();
        let (sent_width, sent_height) = self.sent_target;
        if (width - sent_width).abs() > TARGET_RESIZE_THRESHOLD
            || (height - sent_height).abs() > TARGET_RESIZE_THRESHOLD
        {
            self.sent_target = (width, height);
            let scale = window.scale_factor();
            self.playback
                .set_target_size(((width * scale) as u32, (height * scale) as u32));
        }
        let exporting = self.job.is_some();
        if self.playing
            || self.awaiting_frame
            || self.scrubbing
            || exporting
            || !self.filmstrip.complete()
        {
            window.request_animation_frame();
        }
    }

    fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let shift = keystroke.modifiers.shift;
        match keystroke.key.as_str() {
            "space" | "k" => self.toggle_playing(),
            "left" => self.seek_relative(if shift { -1.0 } else { -SEEK_STEP_SECONDS }),
            "right" => self.seek_relative(if shift { 1.0 } else { SEEK_STEP_SECONDS }),
            "," => self.step_frame(-1.0),
            "." => self.step_frame(1.0),
            "f" => window.toggle_fullscreen(),
            "m" => self.muted = !self.muted,
            "d" => self.debug = !self.debug,
            "escape" if self.job.is_some() => {}
            "escape" if self.mode != Mode::Watch => self.enter_mode(Mode::Watch, cx),
            "escape" => cx.emit(PlayerEvent::Back),
            _ => return,
        }
        cx.notify();
    }

    fn video_surface(&self) -> impl IntoElement + use<> {
        let video_size = self.video_size.clone();
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(
                canvas(
                    move |bounds: Bounds<Pixels>, _, _| {
                        video_size
                            .set((f32::from(bounds.size.width), f32::from(bounds.size.height)))
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .children(self.current_image.clone().map(|image| {
                img(image)
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .object_fit(ObjectFit::Contain)
            }))
    }

    fn timeline_element(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let position_fraction = (self.position() / self.info.duration_seconds.max(0.001)) as f32;
        let selection = (self.mode == Mode::Trim).then(|| self.selection.fractions());
        let view = cx.entity();
        timeline(
            "player-timeline",
            &self.filmstrip,
            TimelineMarks {
                playhead: position_fraction,
                hover: self.hover_fraction,
                selection,
            },
            window,
            cx,
            move |event, cx| {
                view.update(cx, |this, cx| {
                    this.pointer_on_timeline(
                        event.phase,
                        event.fraction(),
                        f32::from(event.size.width),
                    );
                    cx.notify();
                })
            },
        )
    }

    fn top_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .flex_none()
            .h(px(64.0))
            .px_5()
            .flex()
            .items_center()
            .justify_between()
            .bg(theme::background())
            .border_b_1()
            .border_color(theme::border_subtle())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        icon_button("back-to-library", "arrow-left", 40.0)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(PlayerEvent::Back))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(15.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(SharedString::from(self.clip.title.clone())),
                                    )
                                    .child(icon_button("open-file", "file", 28.0).on_click({
                                        let path = self.clip.path.clone();
                                        move |_, _, cx| cx.open_with_system(&path)
                                    }))
                                    .child(icon_button("show-in-folder", "folder", 28.0).on_click(
                                        {
                                            let path = self.clip.path.clone();
                                            move |_, _, cx| cx.reveal_path(&path)
                                        },
                                    ))
                                    .child(icon_button("delete-clip", "trash", 28.0).on_click(
                                        cx.listener(|_, _, _, cx| cx.emit(PlayerEvent::Delete)),
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme::text_muted())
                                    .child(self.subtitle.clone()),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(30.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .rounded(px(8.0))
                    .font_family(theme::MONO_FONT)
                    .text_size(px(12.5))
                    .text_color(theme::text_muted())
                    .child(SharedString::from(self.info.summary())),
            )
    }

    fn debug_overlay(&self) -> impl IntoElement + use<> {
        let stats = self.playback.stats();
        div()
            .absolute()
            .bottom_2()
            .left_2()
            .px_2()
            .py_1()
            .rounded(px(6.0))
            .bg(gpui::rgba(0x000000b3))
            .font_family(theme::MONO_FONT)
            .text_size(px(11.0))
            .child(SharedString::from(format!(
                "present {:.1} fps · decode {:.1} ms · queue {} · decoded {} · seek {:.0} ms · audio {}",
                self.presented_fps,
                stats.average_frame_milliseconds,
                stats.queued,
                stats.decoded_frames,
                self.seek_latency_milliseconds,
                self.audio
                    .position()
                    .map_or("none".to_string(), |seconds| format!(
                        "{:+.0} ms",
                        (seconds - self.position()) * 1000.0
                    ))
            )))
    }

    fn play_pause_button(&self, size: f32, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .id("play-pause")
            .size(px(size))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(theme::accent())
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_playing();
                cx.notify();
            }))
            .child(icon(
                if self.playing { "pause" } else { "play" },
                18.0,
                theme::on_accent(),
            ))
    }

    fn time_readout(&self) -> impl IntoElement + use<> {
        div()
            .flex()
            .whitespace_nowrap()
            .font_family(theme::MONO_FONT)
            .text_size(px(13.0))
            .child(format_timecode(self.position()))
            .child(
                div()
                    .text_color(theme::text_dim())
                    .child(SharedString::from(format!(
                        " / {}",
                        format_timecode(self.info.duration_seconds)
                    ))),
            )
    }

    fn controls(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let view = cx.entity();
        div()
            .h(px(86.0))
            .flex_none()
            .px_6()
            .flex()
            .items_center()
            .gap_4()
            .child(
                icon_button("previous-clip", "skip-back", 36.0)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(PlayerEvent::Navigate(-1)))),
            )
            .child(self.play_pause_button(48.0, cx))
            .child(
                icon_button("next-clip", "skip-forward", 36.0)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(PlayerEvent::Navigate(1)))),
            )
            .child(div().w(px(16.0)))
            .child(
                icon_button("mute", "volume", 36.0).on_click(cx.listener(|this, _, _, cx| {
                    this.muted = !this.muted;
                    cx.notify();
                })),
            )
            .child(slider(
                "volume",
                if self.muted { 0.0 } else { self.volume },
                84.0,
                window,
                cx,
                move |fraction, cx| {
                    view.update(cx, |this, cx| {
                        this.volume = fraction;
                        this.muted = false;
                        cx.notify();
                    })
                },
            ))
            .child(div().ml_4().child(self.time_readout()))
            .child(div().flex_1())
            .child(
                div()
                    .id("speed")
                    .px_3()
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .rounded(px(8.0))
                    .font_family(theme::MONO_FONT)
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::surface_hover()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.cycle_speed();
                        cx.notify();
                    }))
                    .child(SharedString::from(format!("{}×", SPEEDS[self.speed_index]))),
            )
            .child(
                icon_button("fullscreen", "fullscreen", 36.0)
                    .on_click(|_, window, _| window.toggle_fullscreen()),
            )
            .child(div().w(px(1.0)).h(px(28.0)).bg(theme::border()))
            .child(
                button("compress", "Compress", Some("zap"), ButtonKind::Secondary)
                    .on_click(cx.listener(|this, _, _, cx| this.enter_mode(Mode::Compress, cx))),
            )
            .child(
                button(
                    "trim-and-export",
                    "Trim & export",
                    Some("scissors"),
                    ButtonKind::Primary,
                )
                .on_click(cx.listener(|this, _, _, cx| this.enter_mode(Mode::Trim, cx))),
            )
    }

    fn hover_preview(&self, fraction: f32) -> impl IntoElement + use<> {
        let tile = self.filmstrip.tile_at(f64::from(fraction));
        div()
            .absolute()
            .bottom(px(TIMELINE_HEIGHT + 10.0))
            .left(px(preview_left(
                fraction,
                self.timeline_width,
                HOVER_PREVIEW_WIDTH,
            )))
            .w(px(HOVER_PREVIEW_WIDTH))
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .child(
                div()
                    .w_full()
                    .h(px(108.0))
                    .rounded(px(6.0))
                    .overflow_hidden()
                    .border_2()
                    .border_color(gpui::white())
                    .bg(theme::surface())
                    .children(
                        tile.map(|image| img(image).size_full().object_fit(ObjectFit::Cover)),
                    ),
            )
            .child(
                div()
                    .px_2()
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .rounded(px(5.0))
                    .bg(theme::text())
                    .text_color(theme::on_accent())
                    .font_family(theme::MONO_FONT)
                    .text_size(px(11.0))
                    .child(format_timecode(
                        f64::from(fraction) * self.info.duration_seconds,
                    )),
            )
    }

    fn watch_layout(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let hover = self.hover_fraction;
        let timeline_element = self.timeline_element(window, cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(gpui::black())
            .child(self.top_overlay(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.video_surface())
                    .when(self.debug, |this| this.child(self.debug_overlay())),
            )
            .child(
                div()
                    .flex_none()
                    .bg(theme::background())
                    .child(div().h(px(1.0)).w_full().bg(theme::border_subtle()))
                    .child(
                        div()
                            .relative()
                            .mx_6()
                            .mt_4()
                            .child(timeline_element)
                            .children(hover.map(|fraction| self.hover_preview(fraction))),
                    )
                    .child(self.controls(window, cx)),
            )
    }
}

fn back_pill(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .h(px(44.0))
        .pl_3()
        .pr_4()
        .flex()
        .items_center()
        .gap_2()
        .rounded_full()
        .bg(gpui::rgba(0x0a0b0dcc))
        .border_1()
        .border_color(theme::border())
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(|style| style.bg(theme::surface_raised()))
        .on_click(on_click)
        .child(icon("arrow-left", 16.0, theme::text()))
        .child(label)
}

fn initial_target(window: &Window) -> (u32, u32) {
    let viewport = window.viewport_size();
    let scale = window.scale_factor();
    let width = f32::from(viewport.width) * scale;
    let height = (f32::from(viewport.height) - 200.0).max(240.0) * scale;
    (width as u32, height as u32)
}

impl Render for PlayerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.advance(window);
        let body = match self.mode {
            Mode::Watch => self.watch_layout(window, cx).into_any_element(),
            Mode::Trim | Mode::Compress => self.workspace_layout(window, cx).into_any_element(),
        };
        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_key(event, window, cx)
            }))
            .relative()
            .size_full()
            .child(body)
            .children(self.job.as_ref().map(|_| self.export_dialog(cx)))
    }
}
