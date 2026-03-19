// SPDX-License-Identifier: MIT OR Apache-2.0

//! MP4 screen recording via FFmpeg.
//!
//! Video is encoded as H.264 (YUV420P, CRF 23, ultrafast preset) and audio —
//! when available — as AAC 128 kbps stereo at 44 100 Hz.  The recording runs
//! in a dedicated OS thread: the caller pushes [`DecodedFrame`] and
//! [`DecodedAudio`] values through bounded channels and stops the session by
//! dropping (or calling [`RecorderHandle::stop`] on) the handle.

use {
    crate::{
        capture::CaptureEvent,
        decoder::{DecodedAudio, DecodedFrame},
    },
    chrono::Local,
    crossbeam_channel::{Receiver, Sender, TryRecvError, bounded, select},
    ffmpeg_next::{
        self as ffmpeg,
        ChannelLayout,
        Dictionary,
        Rational,
        codec::{encoder, packet::Packet},
        format::{self, Pixel, flag::Flags as FmtFlags},
        frame::{Audio as AudioFrame, Video as VideoFrame},
        software::{
            resampling::context::Context as SwrContext,
            scaling::{context::Context as SwsContext, flag::Flags as SwsFlags},
        },
    },
    std::{path::PathBuf, sync::Arc, thread},
};

const VIDEO_CHANNEL_CAP: usize = 30;
const AUDIO_CHANNEL_CAP: usize = 60;
const AUDIO_SAMPLE_RATE: u32 = 44_100;

#[derive(Clone, Copy)]
struct AudioOutputConfig {
    sample_rate: u32,
    sample_format: ffmpeg_next::format::Sample,
    channel_layout: ChannelLayout,
}

/// Configuration for a new recording session.
pub struct RecorderConfig {
    /// Width of the incoming video frames in pixels.
    pub width: u32,
    /// Height of the incoming video frames in pixels.
    pub height: u32,
    /// Directory where the output MP4 file will be written.
    pub save_dir: PathBuf,
    /// Number of 90° clockwise rotations to apply (0–3), matching the UI display orientation.
    pub rotation: u32,
}

/// Handle to a running recording session.
///
/// Drop this value (or call [`RecorderHandle::stop`]) to signal the recording
/// thread to flush and close the output file.  The final path is delivered
/// through the `event_tx` channel as [`CaptureEvent::RecordingSaved`] on
/// success, or [`CaptureEvent::RecordingError`] on failure.
pub struct RecorderHandle {
    stop_tx: Sender<()>,
    video_tx: Sender<Arc<DecodedFrame>>,
    audio_tx: Option<Sender<DecodedAudio>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl RecorderHandle {
    pub fn start(config: RecorderConfig, event_tx: Sender<CaptureEvent>, has_audio: bool) -> Self {
        let (stop_tx, stop_rx) = bounded::<()>(1);
        let (video_tx, video_rx) = bounded::<Arc<DecodedFrame>>(VIDEO_CHANNEL_CAP);
        let (audio_tx_opt, audio_rx_opt) = if has_audio {
            let (tx, rx) = bounded::<DecodedAudio>(AUDIO_CHANNEL_CAP);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let audio_tx_clone = audio_tx_opt.clone();
        let handle = thread::spawn(move || {
            let result = run_recorder(config, video_rx, audio_rx_opt, stop_rx);
            let evt = match result {
                Ok(path) => CaptureEvent::RecordingSaved(path),
                Err(e) => CaptureEvent::RecordingError(e),
            };
            let _ = event_tx.send(evt);
        });

        Self {
            stop_tx,
            video_tx,
            audio_tx: audio_tx_clone,
            thread: Some(handle),
        }
    }

    /// Returns a sender for pushing video frames into the recording pipeline.
    pub fn video_sender(&self) -> Sender<Arc<DecodedFrame>> { self.video_tx.clone() }

    /// Returns a sender for pushing decoded audio into the recording pipeline,
    /// or `None` if the session was started without audio (`has_audio = false`).
    pub fn audio_sender(&self) -> Option<Sender<DecodedAudio>> { self.audio_tx.clone() }

    /// Signals the recording thread to stop and blocks until it exits.
    ///
    /// The `Drop` implementation provides the same guarantee automatically, so calling
    /// `stop()` is optional — it exists as an explicit, readable shutdown point.
    pub fn stop(self) { drop(self); }
}

impl Drop for RecorderHandle {
    fn drop(&mut self) {
        let _ = self.stop_tx.try_send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn run_recorder(
    config: RecorderConfig,
    video_rx: Receiver<Arc<DecodedFrame>>,
    audio_rx: Option<Receiver<DecodedAudio>>,
    stop_rx: Receiver<()>,
) -> Result<PathBuf, String> {
    ffmpeg::init().map_err(|e| format!("Failed to initialize FFmpeg for recording: {e}"))?;

    let filename = format!(
        "saide_recording_{}.mp4",
        Local::now().format("%Y%m%d_%H%M%S")
    );
    let path = config.save_dir.join(&filename);

    let mut octx = format::output(&path)
        .map_err(|e| format!("Failed to open output file {:?}: {}", path, e))?;

    let needs_global_header = octx.format().flags().contains(FmtFlags::GLOBAL_HEADER);

    let (enc_w, enc_h) = if config.rotation % 2 == 1 {
        (config.height, config.width)
    } else {
        (config.width, config.height)
    };

    let mut video_encoder = open_video_encoder(&mut octx, enc_w, enc_h, needs_global_header)?;
    let video_stream_idx = octx.nb_streams() as usize - 1;

    let mut audio_enc_and_idx: Option<(encoder::audio::Encoder, usize, AudioOutputConfig)> =
        if audio_rx.is_some() {
            let (enc, output_config) = open_audio_encoder(&mut octx, needs_global_header)?;
            let idx = octx.nb_streams() as usize - 1;
            Some((enc, idx, output_config))
        } else {
            None
        };

    octx.write_header()
        .map_err(|e| format!("Failed to write MP4 header: {}", e))?;

    let video_enc_tb = video_encoder.time_base();
    let mut video_base_pts: Option<i64> = None;
    let mut audio_pts: i64 = 0;
    let mut sws_ctx: Option<SwsContext> = None;
    let mut sws_src_key: Option<(Pixel, u32, u32)> = None;
    let mut swr_ctx: Option<SwrContext> = None;
    let audio_frame_size = audio_enc_and_idx
        .as_ref()
        .map(|(enc, ..)| enc.frame_size() as usize)
        .unwrap_or(1024);
    let mut audio_buf_l: Vec<f32> = Vec::new();
    let mut audio_buf_r: Vec<f32> = Vec::new();

    let rotation = config.rotation;
    let mut video_active = true;
    let mut audio_active = audio_rx.is_some();
    let mut stop_requested = false;

    loop {
        if !video_active && !audio_active {
            break;
        }

        match (video_active, audio_active) {
            (true, true) => {
                let arx = audio_rx
                    .as_ref()
                    .expect("audio receiver should exist while audio is active");
                select! {
                    recv(stop_rx) -> _ => {
                        stop_requested = true;
                        break;
                    },
                    recv(video_rx) -> msg => {
                        match msg {
                            Ok(frame) => {
                                if let Err(e) = encode_video_frame(
                                    &frame, &mut video_encoder, &mut octx,
                                    &mut sws_ctx, &mut sws_src_key, video_stream_idx, video_enc_tb,
                                    &mut video_base_pts, rotation,
                                ) {
                                    tracing::warn!("Video encode error: {}", e);
                                }
                            }
                            Err(_) => {
                                video_active = false;
                            }
                        }
                    },
                    recv(arx) -> msg => {
                        match msg {
                            Ok(audio) => {
                                if let Some((ref mut aenc, aidx, output_config)) = audio_enc_and_idx {
                                    if let Err(e) = resample_and_buffer(
                                        &audio,
                                        &mut swr_ctx,
                                        &mut audio_buf_l,
                                        &mut audio_buf_r,
                                        output_config,
                                    ) {
                                        tracing::warn!("Audio resample error: {}", e);
                                    } else if let Err(e) = encode_audio_buffer(
                                        aenc,
                                        &mut octx,
                                        &mut audio_buf_l,
                                        &mut audio_buf_r,
                                        audio_frame_size,
                                        aidx,
                                        &mut audio_pts,
                                        output_config,
                                    ) {
                                        tracing::warn!("Audio encode error: {}", e);
                                    }
                                }
                            }
                            Err(_) => {
                                audio_active = false;
                            }
                        }
                    },
                }
            }
            (true, false) => {
                select! {
                    recv(stop_rx) -> _ => {
                        stop_requested = true;
                        break;
                    },
                    recv(video_rx) -> msg => {
                        match msg {
                            Ok(frame) => {
                                if let Err(e) = encode_video_frame(
                                    &frame, &mut video_encoder, &mut octx,
                                    &mut sws_ctx, &mut sws_src_key, video_stream_idx, video_enc_tb,
                                    &mut video_base_pts, rotation,
                                ) {
                                    tracing::warn!("Video encode error: {}", e);
                                }
                            }
                            Err(_) => {
                                video_active = false;
                            }
                        }
                    },
                }
            }
            (false, true) => {
                let arx = audio_rx
                    .as_ref()
                    .expect("audio receiver should exist while audio is active");
                select! {
                    recv(stop_rx) -> _ => {
                        stop_requested = true;
                        break;
                    },
                    recv(arx) -> msg => {
                        match msg {
                            Ok(audio) => {
                                if let Some((ref mut aenc, aidx, output_config)) = audio_enc_and_idx {
                                    if let Err(e) = resample_and_buffer(
                                        &audio,
                                        &mut swr_ctx,
                                        &mut audio_buf_l,
                                        &mut audio_buf_r,
                                        output_config,
                                    ) {
                                        tracing::warn!("Audio resample error: {}", e);
                                    } else if let Err(e) = encode_audio_buffer(
                                        aenc,
                                        &mut octx,
                                        &mut audio_buf_l,
                                        &mut audio_buf_r,
                                        audio_frame_size,
                                        aidx,
                                        &mut audio_pts,
                                        output_config,
                                    ) {
                                        tracing::warn!("Audio encode error: {}", e);
                                    }
                                }
                            }
                            Err(_) => {
                                audio_active = false;
                            }
                        }
                    },
                }
            }
            (false, false) => break,
        }
    }

    if stop_requested {
        loop {
            let mut made_progress = false;

            match video_rx.try_recv() {
                Ok(frame) => {
                    made_progress = true;
                    if let Err(e) = encode_video_frame(
                        &frame,
                        &mut video_encoder,
                        &mut octx,
                        &mut sws_ctx,
                        &mut sws_src_key,
                        video_stream_idx,
                        video_enc_tb,
                        &mut video_base_pts,
                        rotation,
                    ) {
                        tracing::warn!("Video encode error while draining stop queue: {e}");
                    }
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
            }

            if audio_active && let Some(ref arx) = audio_rx {
                match arx.try_recv() {
                    Ok(audio) => {
                        made_progress = true;
                        if let Some((ref mut aenc, aidx, output_config)) = audio_enc_and_idx {
                            if let Err(e) = resample_and_buffer(
                                &audio,
                                &mut swr_ctx,
                                &mut audio_buf_l,
                                &mut audio_buf_r,
                                output_config,
                            ) {
                                tracing::warn!(
                                    "Audio resample error while draining stop queue: {e}"
                                );
                            } else if let Err(e) = encode_audio_buffer(
                                aenc,
                                &mut octx,
                                &mut audio_buf_l,
                                &mut audio_buf_r,
                                audio_frame_size,
                                aidx,
                                &mut audio_pts,
                                output_config,
                            ) {
                                tracing::warn!("Audio encode error while draining stop queue: {e}");
                            }
                        }
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        audio_active = false;
                    }
                }
            }

            if !made_progress {
                break;
            }
        }
    }

    drain_encoder(
        &mut video_encoder,
        &mut octx,
        video_stream_idx,
        video_enc_tb,
    )?;
    if let Some((ref mut aenc, aidx, output_config)) = audio_enc_and_idx {
        flush_resampler_buffer(
            &mut swr_ctx,
            aenc,
            &mut octx,
            &mut audio_buf_l,
            &mut audio_buf_r,
            audio_frame_size,
            aidx,
            &mut audio_pts,
            output_config,
        )?;
        flush_audio_buffer(
            aenc,
            &mut octx,
            &mut audio_buf_l,
            &mut audio_buf_r,
            audio_frame_size,
            aidx,
            &mut audio_pts,
            output_config,
        )?;
        let audio_enc_tb = aenc.time_base();
        drain_encoder(aenc, &mut octx, aidx, audio_enc_tb)?;
    }

    octx.write_trailer()
        .map_err(|e| format!("Failed to write MP4 trailer: {}", e))?;

    Ok(path)
}

fn open_video_encoder(
    octx: &mut format::context::Output,
    width: u32,
    height: u32,
    global_header: bool,
) -> Result<encoder::video::Encoder, String> {
    let codec = encoder::find(ffmpeg_next::codec::Id::H264)
        .ok_or_else(|| "H264 encoder not found".to_string())?;

    let mut video = ffmpeg_next::codec::context::Context::new_with_codec(codec)
        .encoder()
        .video()
        .map_err(|e| format!("Failed to create video encoder context: {}", e))?;

    video.set_width(width);
    video.set_height(height);
    video.set_format(Pixel::YUV420P);
    video.set_time_base(Rational::new(1, 1_000_000));
    video.set_frame_rate(Some(Rational::new(60, 1)));

    if global_header {
        unsafe {
            (*video.as_mut_ptr()).flags |= ffmpeg_next::ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32;
        }
    }

    let mut dict = Dictionary::new();
    dict.set("crf", "23");
    dict.set("preset", "ultrafast");

    let enc = video
        .open_with(dict)
        .map_err(|e| format!("Failed to open H264 encoder: {}", e))?;

    let mut stream = octx
        .add_stream(codec)
        .map_err(|e| format!("Failed to add video stream: {}", e))?;
    stream.set_time_base(Rational::new(1, 1_000_000));
    unsafe {
        ffmpeg_next::ffi::avcodec_parameters_from_context(
            (*stream.as_mut_ptr()).codecpar,
            enc.as_ptr(),
        );
    }

    Ok(enc)
}

fn open_audio_encoder(
    octx: &mut format::context::Output,
    global_header: bool,
) -> Result<(encoder::audio::Encoder, AudioOutputConfig), String> {
    let codec = encoder::find(ffmpeg_next::codec::Id::AAC)
        .ok_or_else(|| "AAC encoder not found".to_string())?;
    let audio_codec = codec
        .audio()
        .map_err(|e| format!("Failed to inspect AAC encoder capabilities: {e}"))?;

    let sample_rate = audio_codec
        .rates()
        .and_then(|mut rates| {
            rates
                .find(|rate| *rate as u32 == AUDIO_SAMPLE_RATE)
                .or_else(|| rates.next())
        })
        .map(|rate| rate as u32)
        .unwrap_or(AUDIO_SAMPLE_RATE);
    let sample_format = match audio_codec.formats() {
        Some(mut formats) => formats
            .find(|format| {
                *format
                    == ffmpeg_next::format::Sample::F32(ffmpeg_next::format::sample::Type::Planar)
            })
            .ok_or_else(|| "AAC encoder does not support planar f32 samples".to_string())?,
        None => ffmpeg_next::format::Sample::F32(ffmpeg_next::format::sample::Type::Planar),
    };
    let channel_layout = audio_codec
        .channel_layouts()
        .map(|layouts| layouts.best(ChannelLayout::STEREO.channels()))
        .unwrap_or(ChannelLayout::STEREO);
    if channel_layout != ChannelLayout::STEREO {
        return Err("AAC encoder does not support stereo output".to_string());
    }
    let output_config = AudioOutputConfig {
        sample_rate,
        sample_format,
        channel_layout,
    };

    let mut audio = ffmpeg_next::codec::context::Context::new_with_codec(codec)
        .encoder()
        .audio()
        .map_err(|e| format!("Failed to create audio encoder context: {}", e))?;

    audio.set_rate(sample_rate as i32);
    audio.set_format(sample_format);
    audio.set_channel_layout(channel_layout);
    audio.set_time_base(Rational::new(1, sample_rate as i32));
    audio.set_bit_rate(128_000);

    if global_header {
        audio.set_flags(ffmpeg_next::codec::flag::Flags::GLOBAL_HEADER);
    }

    let mut stream = octx
        .add_stream(codec)
        .map_err(|e| format!("Failed to add audio stream: {}", e))?;
    stream.set_time_base(Rational::new(1, sample_rate as i32));

    let enc = audio
        .open_as(codec)
        .map_err(|e| format!("Failed to open AAC encoder: {}", e))?;

    stream.set_parameters(&enc);

    Ok((enc, output_config))
}

#[allow(clippy::too_many_arguments)]
fn encode_video_frame(
    frame: &DecodedFrame,
    encoder: &mut encoder::video::Encoder,
    octx: &mut format::context::Output,
    sws_ctx: &mut Option<SwsContext>,
    sws_src_key: &mut Option<(Pixel, u32, u32)>,
    stream_idx: usize,
    enc_time_base: Rational,
    base_pts: &mut Option<i64>,
    rotation: u32,
) -> Result<(), String> {
    let src_fmt = frame.format;
    let w = frame.width;
    let h = frame.height;

    let key = (src_fmt, w, h);
    if sws_src_key.as_ref() != Some(&key) {
        *sws_ctx = None;
        *sws_src_key = Some(key);
    }

    if sws_ctx.is_none() {
        *sws_ctx = Some(
            SwsContext::get(src_fmt, w, h, Pixel::YUV420P, w, h, SwsFlags::BILINEAR)
                .map_err(|e| format!("Failed to create swscale context: {}", e))?,
        );
    }

    let mut src_frame = VideoFrame::new(src_fmt, w, h);
    fill_video_frame(&mut src_frame, &frame.data, src_fmt, w, h)?;

    let mut yuv_frame = VideoFrame::new(Pixel::YUV420P, w, h);

    sws_ctx
        .as_mut()
        .unwrap()
        .run(&src_frame, &mut yuv_frame)
        .map_err(|e| format!("swscale run failed: {}", e))?;

    let mut final_frame = if !rotation.is_multiple_of(4) {
        rotate_yuv420p(&yuv_frame, w, h, rotation)?
    } else {
        yuv_frame
    };

    let bp = base_pts.get_or_insert(frame.pts);
    let relative_pts = frame.pts - *bp;

    unsafe {
        (*final_frame.as_mut_ptr()).pts = relative_pts;
    }

    encoder
        .send_frame(&final_frame)
        .map_err(|e| format!("send_frame failed: {}", e))?;

    drain_ready_packets(encoder, octx, stream_idx, enc_time_base)
}

/// Resamples the incoming audio chunk and appends it to the planar output buffers.
///
/// Input is interleaved f32 samples; output is planar f32 (left and right channels separated).
/// The `SwrContext` maintains sample-rate conversion state and accepts arbitrary input chunk sizes.
fn resample_and_buffer(
    audio: &DecodedAudio,
    swr_ctx: &mut Option<SwrContext>,
    buf_l: &mut Vec<f32>,
    buf_r: &mut Vec<f32>,
    output_config: AudioOutputConfig,
) -> Result<(), String> {
    let src_rate = audio.sample_rate as i32;
    let src_ch = audio.channels as i32;
    let src_layout = if src_ch >= 2 {
        ChannelLayout::STEREO
    } else {
        ChannelLayout::MONO
    };

    if swr_ctx.is_none() {
        *swr_ctx = Some(
            SwrContext::get(
                ffmpeg_next::format::Sample::F32(ffmpeg_next::format::sample::Type::Packed),
                src_layout,
                src_rate as u32,
                output_config.sample_format,
                output_config.channel_layout,
                output_config.sample_rate,
            )
            .map_err(|e| format!("Failed to create swresample context: {}", e))?,
        );
    }

    let samples_per_ch = audio.samples.len() / audio.channels.max(1) as usize;

    let mut src_frame = AudioFrame::new(
        ffmpeg_next::format::Sample::F32(ffmpeg_next::format::sample::Type::Packed),
        samples_per_ch,
        src_layout,
    );
    src_frame.set_rate(audio.sample_rate);
    {
        let plane = src_frame.data_mut(0);
        let src_bytes: &[u8] = bytemuck::cast_slice(&audio.samples);
        let copy_len = src_bytes.len().min(plane.len());
        plane[..copy_len].copy_from_slice(&src_bytes[..copy_len]);
    }

    let max_out = samples_per_ch * 2 + 1024;
    let mut dst_frame = AudioFrame::new(
        output_config.sample_format,
        max_out,
        output_config.channel_layout,
    );
    dst_frame.set_rate(output_config.sample_rate);

    swr_ctx
        .as_mut()
        .unwrap()
        .run(&src_frame, &mut dst_frame)
        .map_err(|e| format!("swresample run failed: {}", e))?;

    let out_samples = dst_frame.samples();
    if out_samples > 0 {
        let out_l = dst_frame.plane::<f32>(0);
        let out_r = dst_frame.plane::<f32>(1);
        buf_l.extend_from_slice(out_l);
        buf_r.extend_from_slice(out_r);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn flush_resampler_buffer(
    swr_ctx: &mut Option<SwrContext>,
    encoder: &mut encoder::audio::Encoder,
    octx: &mut format::context::Output,
    buf_l: &mut Vec<f32>,
    buf_r: &mut Vec<f32>,
    frame_size: usize,
    stream_idx: usize,
    pts: &mut i64,
    output_config: AudioOutputConfig,
) -> Result<(), String> {
    let Some(swr) = swr_ctx.as_mut() else {
        return Ok(());
    };

    loop {
        let pending_samples = swr
            .delay()
            .map(|delay| delay.output.max(0) as usize)
            .unwrap_or(0);
        if pending_samples == 0 {
            break;
        }

        let mut delayed_frame = AudioFrame::new(
            output_config.sample_format,
            pending_samples,
            output_config.channel_layout,
        );
        delayed_frame.set_rate(output_config.sample_rate);

        let remaining = swr
            .flush(&mut delayed_frame)
            .map_err(|e| format!("swresample flush failed: {e}"))?;
        let out_samples = delayed_frame.samples();
        if out_samples == 0 {
            break;
        }

        let out_l = delayed_frame.plane::<f32>(0);
        let out_r = delayed_frame.plane::<f32>(1);
        buf_l.extend_from_slice(out_l);
        buf_r.extend_from_slice(out_r);
        encode_audio_buffer(
            encoder,
            octx,
            buf_l,
            buf_r,
            frame_size,
            stream_idx,
            pts,
            output_config,
        )?;

        if remaining.is_none() {
            break;
        }
    }

    Ok(())
}

/// Drains complete `frame_size`-sample chunks from the buffers, encodes them, and writes to output.
///
/// Each iteration consumes exactly `frame_size` samples (1024 for AAC), constructs a planar
/// `AudioFrame`, and sends it to the encoder.
#[allow(clippy::too_many_arguments)]
fn encode_audio_buffer(
    encoder: &mut encoder::audio::Encoder,
    octx: &mut format::context::Output,
    buf_l: &mut Vec<f32>,
    buf_r: &mut Vec<f32>,
    frame_size: usize,
    stream_idx: usize,
    pts: &mut i64,
    output_config: AudioOutputConfig,
) -> Result<(), String> {
    while buf_l.len() >= frame_size && buf_r.len() >= frame_size {
        let chunk_l: Vec<f32> = buf_l.drain(..frame_size).collect();
        let chunk_r: Vec<f32> = buf_r.drain(..frame_size).collect();

        let mut enc_frame = AudioFrame::new(
            output_config.sample_format,
            frame_size,
            output_config.channel_layout,
        );
        enc_frame.set_rate(output_config.sample_rate);
        enc_frame.set_pts(Some(*pts));

        enc_frame.plane_mut::<f32>(0).copy_from_slice(&chunk_l);
        enc_frame.plane_mut::<f32>(1).copy_from_slice(&chunk_r);

        *pts += frame_size as i64;

        encoder
            .send_frame(&enc_frame)
            .map_err(|e| format!("audio send_frame failed: {}", e))?;

        drain_ready_packets(
            encoder,
            octx,
            stream_idx,
            Rational::new(1, output_config.sample_rate as i32),
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn flush_audio_buffer(
    encoder: &mut encoder::audio::Encoder,
    octx: &mut format::context::Output,
    buf_l: &mut Vec<f32>,
    buf_r: &mut Vec<f32>,
    frame_size: usize,
    stream_idx: usize,
    pts: &mut i64,
    output_config: AudioOutputConfig,
) -> Result<(), String> {
    if buf_l.is_empty() || buf_r.is_empty() {
        return Ok(());
    }

    let frame_samples = frame_size.max(buf_l.len()).max(buf_r.len());
    let mut padded_l = vec![0.0f32; frame_samples];
    let mut padded_r = vec![0.0f32; frame_samples];

    padded_l[..buf_l.len()].copy_from_slice(buf_l);
    padded_r[..buf_r.len()].copy_from_slice(buf_r);

    buf_l.clear();
    buf_r.clear();

    let mut enc_frame = AudioFrame::new(
        output_config.sample_format,
        frame_samples,
        output_config.channel_layout,
    );
    enc_frame.set_rate(output_config.sample_rate);
    enc_frame.set_pts(Some(*pts));

    enc_frame.plane_mut::<f32>(0).copy_from_slice(&padded_l);
    enc_frame.plane_mut::<f32>(1).copy_from_slice(&padded_r);

    *pts += frame_samples as i64;

    encoder
        .send_frame(&enc_frame)
        .map_err(|e| format!("audio flush send_frame failed: {}", e))?;

    drain_ready_packets(
        encoder,
        octx,
        stream_idx,
        Rational::new(1, output_config.sample_rate as i32),
    )
}

fn drain_ready_packets(
    encoder: &mut encoder::Encoder,
    octx: &mut format::context::Output,
    stream_idx: usize,
    time_base: Rational,
) -> Result<(), String> {
    let mut pkt = Packet::empty();
    loop {
        match encoder.receive_packet(&mut pkt) {
            Ok(()) => {
                pkt.set_stream(stream_idx);
                pkt.rescale_ts(time_base, octx.stream(stream_idx).unwrap().time_base());
                pkt.write_interleaved(octx)
                    .map_err(|e| format!("write_interleaved failed: {}", e))?;
            }
            Err(ffmpeg_next::Error::Other {
                errno: ffmpeg_next::error::EAGAIN,
            }) => break,
            Err(ffmpeg_next::Error::Eof) => break,
            Err(e) => return Err(format!("receive_packet failed: {}", e)),
        }
    }
    Ok(())
}

fn drain_encoder(
    encoder: &mut encoder::Encoder,
    octx: &mut format::context::Output,
    stream_idx: usize,
    enc_time_base: Rational,
) -> Result<(), String> {
    encoder
        .send_eof()
        .map_err(|e| format!("send_eof failed: {}", e))?;

    let stream_tb = octx.stream(stream_idx).unwrap().time_base();
    let mut pkt = Packet::empty();
    while encoder.receive_packet(&mut pkt).is_ok() {
        pkt.set_stream(stream_idx);
        pkt.rescale_ts(enc_time_base, stream_tb);
        pkt.write_interleaved(octx)
            .map_err(|e| format!("write_interleaved failed while draining encoder: {e}"))?;
    }
    Ok(())
}

fn rotate_yuv420p(src: &VideoFrame, w: u32, h: u32, rotation: u32) -> Result<VideoFrame, String> {
    let rot = rotation % 4;
    let (dst_w, dst_h) = if rot % 2 == 1 { (h, w) } else { (w, h) };
    let mut dst = VideoFrame::new(Pixel::YUV420P, dst_w, dst_h);

    let dst_stride_0 = dst.stride(0);
    let dst_stride_1 = dst.stride(1);
    let dst_stride_2 = dst.stride(2);

    // Y plane — full luma resolution (w × h).
    rotate_plane(
        src.data(0),
        src.stride(0),
        dst.data_mut(0),
        dst_stride_0,
        w,
        h,
        rot,
    );

    // Cb (U) and Cr (V) planes — half luma resolution in both dimensions
    // due to YUV420P 4:2:0 chroma subsampling.
    let cw = w / 2;
    let ch = h / 2;
    rotate_plane(
        src.data(1),
        src.stride(1),
        dst.data_mut(1),
        dst_stride_1,
        cw,
        ch,
        rot,
    );
    rotate_plane(
        src.data(2),
        src.stride(2),
        dst.data_mut(2),
        dst_stride_2,
        cw,
        ch,
        rot,
    );

    Ok(dst)
}

fn rotate_plane(
    src: &[u8],
    src_stride: usize,
    dst: &mut [u8],
    dst_stride: usize,
    w: u32,
    h: u32,
    rotation: u32,
) {
    let w = w as usize;
    let h = h as usize;
    match rotation % 4 {
        1 => {
            // 90° CW: dst(x, h-1-y) = src(y, x), dst dims = (h, w)
            for y in 0..h {
                for x in 0..w {
                    dst[x * dst_stride + (h - 1 - y)] = src[y * src_stride + x];
                }
            }
        }
        2 => {
            // 180°: dst(w-1-x, h-1-y) = src(y, x), same dims
            for y in 0..h {
                for x in 0..w {
                    dst[(h - 1 - y) * dst_stride + (w - 1 - x)] = src[y * src_stride + x];
                }
            }
        }
        3 => {
            // 270° CW: dst(w-1-x, y) = src(y, x), dst dims = (h, w)
            for y in 0..h {
                for x in 0..w {
                    dst[(w - 1 - x) * dst_stride + y] = src[y * src_stride + x];
                }
            }
        }
        _ => {
            for y in 0..h {
                let s = y * src_stride;
                let d = y * dst_stride;
                dst[d..d + w].copy_from_slice(&src[s..s + w]);
            }
        }
    }
}

fn fill_video_frame(
    dst: &mut VideoFrame,
    src_data: &[u8],
    fmt: Pixel,
    w: u32,
    h: u32,
) -> Result<(), String> {
    match fmt {
        Pixel::NV12 => {
            let y_size = (w * h) as usize;
            let uv_size = y_size / 2;
            if src_data.len() < y_size + uv_size {
                return Err(format!(
                    "NV12 frame too small: {} < {}",
                    src_data.len(),
                    y_size + uv_size
                ));
            }
            let y_stride = dst.stride(0);
            let uv_stride = dst.stride(1);
            let plane0 = dst.data_mut(0);
            for row in 0..h as usize {
                let src_start = row * w as usize;
                let dst_start = row * y_stride;
                plane0[dst_start..dst_start + w as usize]
                    .copy_from_slice(&src_data[src_start..src_start + w as usize]);
            }
            let plane1 = dst.data_mut(1);
            for row in 0..(h / 2) as usize {
                let src_start = y_size + row * w as usize;
                let dst_start = row * uv_stride;
                plane1[dst_start..dst_start + w as usize]
                    .copy_from_slice(&src_data[src_start..src_start + w as usize]);
            }
        }
        Pixel::RGBA | Pixel::BGRA => {
            let stride = dst.stride(0);
            let row_bytes = (w * 4) as usize;
            let plane = dst.data_mut(0);
            for row in 0..h as usize {
                let src_start = row * row_bytes;
                let dst_start = row * stride;
                let end = (dst_start + row_bytes).min(plane.len());
                let len = end - dst_start;
                plane[dst_start..end].copy_from_slice(&src_data[src_start..src_start + len]);
            }
        }
        _ => {
            let plane = dst.data_mut(0);
            let copy_len = src_data.len().min(plane.len());
            plane[..copy_len].copy_from_slice(&src_data[..copy_len]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use {
        super::{RecorderConfig, RecorderHandle, rotate_plane},
        crate::{
            capture::CaptureEvent,
            decoder::{DecodedAudio, DecodedFrame},
        },
        crossbeam_channel::bounded,
        ffmpeg_next::{
            self as ffmpeg,
            codec,
            format::{self as avformat, Pixel},
            media,
        },
        std::{
            fs,
            sync::Arc,
            time::{SystemTime, UNIX_EPOCH},
        },
    };

    fn call(src: &[u8], src_stride: usize, w: u32, h: u32, rotation: u32) -> Vec<u8> {
        let (dst_w, dst_h) = if rotation % 2 == 1 {
            (h as usize, w as usize)
        } else {
            (w as usize, h as usize)
        };
        let mut dst = vec![0u8; dst_w * dst_h];
        rotate_plane(src, src_stride, &mut dst, dst_w, w, h, rotation);
        dst
    }

    #[test]
    fn rotate_plane_rot0_copies_plane() {
        let src = vec![1u8, 2, 3, 4, 5, 6];
        let dst = call(&src, 3, 3, 2, 0);
        assert_eq!(dst, src);
    }

    #[test]
    fn rotate_plane_rot1_90cw() {
        // 2×3 grid (w=2, h=3):
        //  1 2
        //  3 4
        //  5 6
        // After 90° CW → 3×2 grid:
        //  5 3 1
        //  6 4 2
        let src = vec![1u8, 2, 3, 4, 5, 6];
        let dst = call(&src, 2, 2, 3, 1);
        assert_eq!(dst, vec![5, 3, 1, 6, 4, 2]);
    }

    #[test]
    fn rotate_plane_rot2_180() {
        // 2×3 grid → 180° reverses element order
        let src = vec![1u8, 2, 3, 4, 5, 6];
        let dst = call(&src, 2, 2, 3, 2);
        assert_eq!(dst, vec![6, 5, 4, 3, 2, 1]);
    }

    #[test]
    fn rotate_plane_rot3_270cw() {
        // 2×3 grid (w=2, h=3):
        //  1 2
        //  3 4
        //  5 6
        // After 270° CW (= 90° CCW) → 3×2 grid:
        //  2 4 6
        //  1 3 5
        let src = vec![1u8, 2, 3, 4, 5, 6];
        let dst = call(&src, 2, 2, 3, 3);
        assert_eq!(dst, vec![2, 4, 6, 1, 3, 5]);
    }

    #[test]
    fn recorder_writes_audio_stream_to_mp4() {
        let temp_dir = std::env::temp_dir().join(format!(
            "saide-recorder-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).expect("failed to create temp dir");

        let (event_tx, event_rx) = bounded(1);
        let handle = RecorderHandle::start(
            RecorderConfig {
                width: 2,
                height: 2,
                save_dir: temp_dir.clone(),
                rotation: 0,
            },
            event_tx,
            true,
        );

        let video_tx = handle.video_sender();
        let audio_tx = handle
            .audio_sender()
            .expect("recorder should expose audio sender when audio is enabled");

        for i in 0..3 {
            video_tx
                .send(Arc::new(DecodedFrame {
                    data: vec![0, 0, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255],
                    width: 2,
                    height: 2,
                    pts: i * 16_666,
                    format: Pixel::RGBA,
                }))
                .expect("failed to queue video frame for recorder");
        }

        let samples_per_channel = 4_800usize;
        let mut samples = Vec::with_capacity(samples_per_channel * 2);
        for n in 0..samples_per_channel {
            let sample = ((n as f32 / 32.0) * std::f32::consts::TAU).sin() * 0.2;
            samples.push(sample);
            samples.push(sample);
        }
        audio_tx
            .send(DecodedAudio {
                samples,
                sample_rate: 48_000,
                channels: 2,
                pts: 0,
            })
            .expect("failed to queue audio frame for recorder");

        drop(video_tx);
        drop(audio_tx);
        handle.stop();

        let saved_path = match event_rx
            .recv()
            .expect("recorder should send completion event")
        {
            CaptureEvent::RecordingSaved(path) => path,
            CaptureEvent::RecordingError(err) => panic!("recording failed unexpectedly: {err}"),
            other => panic!("unexpected capture event: {other:?}"),
        };

        ffmpeg::init().expect("ffmpeg init should succeed for inspection");
        let ictx = avformat::input(&saved_path).expect("should be able to read generated mp4");
        let mut has_video = false;
        let mut has_audio = false;

        for stream in ictx.streams() {
            let codec_ctx = codec::context::Context::from_parameters(stream.parameters())
                .expect("should read stream parameters");
            match codec_ctx.medium() {
                media::Type::Video => has_video = true,
                media::Type::Audio => has_audio = true,
                _ => {}
            }
        }

        assert!(
            has_video,
            "generated recording should contain a video stream"
        );
        assert!(
            has_audio,
            "generated recording should contain an audio stream"
        );

        fs::remove_file(&saved_path).expect("failed to remove generated mp4");
        fs::remove_dir_all(&temp_dir).expect("failed to remove temp dir");
    }
}
