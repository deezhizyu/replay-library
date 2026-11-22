use std::path::Path;

use anyhow::{Context as _, Result, anyhow};
use ffmpeg::codec::{self, threading};
use ffmpeg::format::{self, context::Input};
use ffmpeg::media::Type;
use ffmpeg::{Packet, Rational, frame};
use ffmpeg_next as ffmpeg;

use crate::library::ClipInfo;

pub struct VideoSource {
    input: Input,
    stream_index: usize,
    decoder: ffmpeg::decoder::Video,
    time_base: Rational,
    frame_rate: f32,
    codec_name: &'static str,
    audio_codec_name: &'static str,
    audio_kbps: u32,
    end_of_stream_sent: bool,
}

impl VideoSource {
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with(path, threading::Config::kind(threading::Type::Frame))
    }

    pub fn open_single_threaded(path: &Path) -> Result<Self> {
        Self::open_with(path, threading::Config::kind(threading::Type::None))
    }

    fn open_with(path: &Path, threading: threading::Config) -> Result<Self> {
        let input =
            format::input(path).with_context(|| format!("failed to open {}", path.display()))?;
        let stream = input
            .streams()
            .best(Type::Video)
            .ok_or_else(|| anyhow!("no video stream"))?;
        let stream_index = stream.index();
        let time_base = stream.time_base();
        let rate = stream.avg_frame_rate();
        let frame_rate = if rate.denominator() == 0 {
            0.0
        } else {
            rate.numerator() as f32 / rate.denominator() as f32
        };
        let codec_name = codec_display_name(stream.parameters().id());
        let (audio_codec_name, audio_kbps) =
            input
                .streams()
                .best(Type::Audio)
                .map_or(("None", 0), |audio| {
                    let parameters = audio.parameters();
                    let kbps = u32::try_from(parameters.bit_rate() / 1000).unwrap_or(0);
                    (audio_codec_display_name(parameters.id()), kbps)
                });
        let mut context = codec::context::Context::from_parameters(stream.parameters())?;
        context.set_threading(threading);
        let decoder = context.decoder().video()?;
        Ok(Self {
            input,
            stream_index,
            decoder,
            time_base,
            frame_rate,
            codec_name,
            audio_codec_name,
            audio_kbps,
            end_of_stream_sent: false,
        })
    }

    pub fn info(&self) -> ClipInfo {
        ClipInfo {
            width: self.decoder.width(),
            height: self.decoder.height(),
            frame_rate: self.frame_rate,
            duration_seconds: self.input.duration() as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE),
            codec: self.codec_name.to_string(),
            bitrate: u64::try_from(self.input.bit_rate()).unwrap_or(0),
            audio_codec: self.audio_codec_name.to_string(),
            audio_kbps: self.audio_kbps,
        }
    }

    pub fn seek_seconds(&mut self, seconds: f64) -> Result<()> {
        let target = (seconds.max(0.0) * f64::from(ffmpeg::ffi::AV_TIME_BASE)) as i64;
        self.input.seek(target, ..target)?;
        self.decoder.flush();
        self.end_of_stream_sent = false;
        Ok(())
    }

    pub fn seconds_of(&self, frame: &frame::Video) -> Option<f64> {
        frame.pts().map(|pts| {
            pts as f64 * f64::from(self.time_base.numerator())
                / f64::from(self.time_base.denominator())
        })
    }

    pub fn next_frame(&mut self) -> Result<Option<frame::Video>> {
        let mut decoded = frame::Video::empty();
        loop {
            match self.decoder.receive_frame(&mut decoded) {
                Ok(()) => return Ok(Some(decoded)),
                Err(ffmpeg::Error::Eof) => return Ok(None),
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => {}
                Err(error) => return Err(error.into()),
            }
            match read_packet(&mut self.input, self.stream_index)? {
                Some(packet) => self.decoder.send_packet(&packet)?,
                None if !self.end_of_stream_sent => {
                    self.decoder.send_eof()?;
                    self.end_of_stream_sent = true;
                }
                None => return Ok(None),
            }
        }
    }
}

fn codec_display_name(identifier: codec::Id) -> &'static str {
    match identifier {
        codec::Id::HEVC => "HEVC",
        codec::Id::H264 => "H.264",
        codec::Id::AV1 => "AV1",
        codec::Id::VP9 => "VP9",
        _ => "Video",
    }
}

fn audio_codec_display_name(identifier: codec::Id) -> &'static str {
    match identifier {
        codec::Id::AAC => "AAC",
        codec::Id::OPUS => "Opus",
        _ => "Audio",
    }
}

pub fn read_packet(input: &mut Input, stream_index: usize) -> Result<Option<Packet>> {
    loop {
        let mut packet = Packet::empty();
        match packet.read(input) {
            Ok(()) if packet.stream() == stream_index => return Ok(Some(packet)),
            Ok(()) | Err(ffmpeg::Error::InvalidData) => {}
            Err(ffmpeg::Error::Eof) => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    }
}
