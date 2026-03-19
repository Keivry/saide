// SPDX-License-Identifier: MIT OR Apache-2.0

use {
    crate::{
        capture::CaptureEvent,
        decoder::{DecodedAudio, DecodedFrame},
    },
    chrono::Local,
    crossbeam_channel::{Receiver, Sender, bounded, select},
    ffmpeg_next::{
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

pub struct RecorderConfig {
    pub width: u32,
    pub height: u32,
    pub save_dir: PathBuf,
    pub rotation: u32,
}

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

    pub fn video_sender(&self) -> Sender<Arc<DecodedFrame>> { self.video_tx.clone() }

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

    let mut audio_enc_and_idx: Option<(encoder::audio::Encoder, usize)> = if audio_rx.is_some() {
        match open_audio_encoder(&mut octx, needs_global_header) {
            Ok(enc) => {
                let idx = octx.nb_streams() as usize - 1;
                Some((enc, idx))
            }
            Err(e) => {
                tracing::warn!("AAC encoder init failed, recording without audio: {e}");
                None
            }
        }
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
        .map(|(enc, _)| enc.frame_size() as usize)
        .unwrap_or(1024);
    let mut audio_buf_l: Vec<f32> = Vec::new();
    let mut audio_buf_r: Vec<f32> = Vec::new();

    let rotation = config.rotation;
    let mut audio_active = audio_rx.is_some();

    loop {
        if audio_active {
            if let Some(ref arx) = audio_rx {
                select! {
                    recv(stop_rx) -> _ => break,
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
                            Err(_) => break,
                        }
                    },
                    recv(arx) -> msg => {
                        match msg {
                            Ok(audio) => {
                                if let Some((ref mut aenc, aidx)) = audio_enc_and_idx {
                                    if let Err(e) = resample_and_buffer(
                                        &audio, &mut swr_ctx, &mut audio_buf_l, &mut audio_buf_r,
                                    ) {
                                        tracing::warn!("Audio resample error: {}", e);
                                    } else if let Err(e) = encode_audio_buffer(
                                        aenc, &mut octx, &mut audio_buf_l, &mut audio_buf_r,
                                        audio_frame_size, aidx, &mut audio_pts,
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
        } else {
            select! {
                recv(stop_rx) -> _ => break,
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
                        Err(_) => break,
                    }
                },
            }
        }
    }

    drain_encoder(
        &mut video_encoder,
        &mut octx,
        video_stream_idx,
        video_enc_tb,
    )?;
    if let Some((ref mut aenc, aidx)) = audio_enc_and_idx {
        let audio_enc_tb = aenc.time_base();
        let _ = drain_encoder(aenc, &mut octx, aidx, audio_enc_tb);
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
) -> Result<encoder::audio::Encoder, String> {
    let codec = encoder::find(ffmpeg_next::codec::Id::AAC)
        .ok_or_else(|| "AAC encoder not found".to_string())?;

    let mut audio = ffmpeg_next::codec::context::Context::new_with_codec(codec)
        .encoder()
        .audio()
        .map_err(|e| format!("Failed to create audio encoder context: {}", e))?;

    audio.set_rate(44100);
    audio.set_format(ffmpeg_next::format::Sample::F32(
        ffmpeg_next::format::sample::Type::Planar,
    ));
    audio.set_channel_layout(ChannelLayout::STEREO);
    audio.set_time_base(Rational::new(1, 44100));
    audio.set_bit_rate(128_000);

    if global_header {
        unsafe {
            (*audio.as_mut_ptr()).flags |= ffmpeg_next::ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32;
        }
    }

    let enc = audio
        .open_as(codec)
        .map_err(|e| format!("Failed to open AAC encoder: {}", e))?;

    let mut stream = octx
        .add_stream(codec)
        .map_err(|e| format!("Failed to add audio stream: {}", e))?;
    stream.set_time_base(Rational::new(1, 44100));
    unsafe {
        ffmpeg_next::ffi::avcodec_parameters_from_context(
            (*stream.as_mut_ptr()).codecpar,
            enc.as_ptr(),
        );
    }

    Ok(enc)
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
                ffmpeg_next::format::Sample::F32(ffmpeg_next::format::sample::Type::Planar),
                ChannelLayout::STEREO,
                44100,
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
        ffmpeg_next::format::Sample::F32(ffmpeg_next::format::sample::Type::Planar),
        max_out,
        ChannelLayout::STEREO,
    );
    dst_frame.set_rate(44100);

    swr_ctx
        .as_mut()
        .unwrap()
        .run(&src_frame, &mut dst_frame)
        .map_err(|e| format!("swresample run failed: {}", e))?;

    let out_samples = dst_frame.samples();
    if out_samples > 0 {
        let byte_len = out_samples * 4;

        let plane0 = dst_frame.data(0);
        let plane1 = dst_frame.data(1);
        let safe_len = byte_len.min(plane0.len()).min(plane1.len());

        let out_l: &[f32] = bytemuck::cast_slice(&plane0[..safe_len]);
        let out_r: &[f32] = bytemuck::cast_slice(&plane1[..safe_len]);
        buf_l.extend_from_slice(out_l);
        buf_r.extend_from_slice(out_r);
    }

    Ok(())
}

/// Drains complete `frame_size`-sample chunks from the buffers, encodes them, and writes to output.
///
/// Each iteration consumes exactly `frame_size` samples (1024 for AAC), constructs a planar
/// `AudioFrame`, and sends it to the encoder.
fn encode_audio_buffer(
    encoder: &mut encoder::audio::Encoder,
    octx: &mut format::context::Output,
    buf_l: &mut Vec<f32>,
    buf_r: &mut Vec<f32>,
    frame_size: usize,
    stream_idx: usize,
    pts: &mut i64,
) -> Result<(), String> {
    while buf_l.len() >= frame_size && buf_r.len() >= frame_size {
        let chunk_l: Vec<f32> = buf_l.drain(..frame_size).collect();
        let chunk_r: Vec<f32> = buf_r.drain(..frame_size).collect();

        let mut enc_frame = AudioFrame::new(
            ffmpeg_next::format::Sample::F32(ffmpeg_next::format::sample::Type::Planar),
            frame_size,
            ChannelLayout::STEREO,
        );
        enc_frame.set_rate(44100);
        enc_frame.set_pts(Some(*pts));

        {
            let plane0 = enc_frame.data_mut(0);
            let src0: &[u8] = bytemuck::cast_slice(&chunk_l);
            let copy_len = src0.len().min(plane0.len());
            plane0[..copy_len].copy_from_slice(&src0[..copy_len]);
        }
        {
            let plane1 = enc_frame.data_mut(1);
            let src1: &[u8] = bytemuck::cast_slice(&chunk_r);
            let copy_len = src1.len().min(plane1.len());
            plane1[..copy_len].copy_from_slice(&src1[..copy_len]);
        }

        *pts += frame_size as i64;

        encoder
            .send_frame(&enc_frame)
            .map_err(|e| format!("audio send_frame failed: {}", e))?;

        drain_ready_packets(encoder, octx, stream_idx, Rational::new(1, 44100))?;
    }
    Ok(())
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
        let _ = pkt.write_interleaved(octx);
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

    rotate_plane(
        src.data(0),
        src.stride(0),
        dst.data_mut(0),
        dst_stride_0,
        w,
        h,
        rot,
    );

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
