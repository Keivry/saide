// SPDX-License-Identifier: MIT OR Apache-2.0

//! PNG screenshot capture.
//!
//! Converts a [`DecodedFrame`] to RGB24 via FFmpeg's `swscale`, applies the
//! requested clockwise rotation through `image::imageops`, and saves the
//! result as a PNG file.  The encoding step runs in a background thread so
//! the UI is never blocked.

use {
    crate::{capture::CaptureEvent, decoder::DecodedFrame},
    chrono::Local,
    crossbeam_channel::Sender,
    ffmpeg_next::{
        format::Pixel,
        software::scaling::{context::Context as SwsContext, flag::Flags},
        util::frame::video::Video as FfmpegFrame,
    },
    std::{
        path::{Path, PathBuf},
        sync::Arc,
        thread,
    },
};

/// Captures the current frame and saves it as a PNG image.
///
/// `rotation` is the number of 90° clockwise rotations (0-3), matching the UI display orientation.
/// The screenshot is encoded in a background thread so the UI is never blocked.
pub fn take_screenshot(
    frame: Arc<DecodedFrame>,
    save_dir: PathBuf,
    event_tx: Sender<CaptureEvent>,
    rotation: u32,
) {
    thread::spawn(move || {
        let result = encode_screenshot(&frame, &save_dir, rotation);
        let event = match result {
            Ok(path) => CaptureEvent::ScreenshotSaved(path),
            Err(e) => CaptureEvent::ScreenshotError(e),
        };
        let _ = event_tx.send(event);
    });
}

fn encode_screenshot(
    frame: &DecodedFrame,
    save_dir: &Path,
    rotation: u32,
) -> Result<PathBuf, String> {
    let src_fmt = frame.format;
    let w = frame.width;
    let h = frame.height;

    let rgb_data = convert_to_rgb24(frame, src_fmt, w, h)?;

    let filename = format!(
        "saide_screenshot_{}.png",
        Local::now().format("%Y%m%d_%H%M%S")
    );
    let path = save_dir.join(&filename);

    let img = image::RgbImage::from_raw(w, h, rgb_data)
        .ok_or_else(|| "Failed to construct RGB image from frame data".to_string())?;

    let rotated: image::RgbImage = match rotation % 4 {
        1 => image::imageops::rotate90(&img),
        2 => image::imageops::rotate180(&img),
        3 => image::imageops::rotate270(&img),
        _ => img,
    };

    rotated
        .save(&path)
        .map_err(|e| format!("Failed to save screenshot to {:?}: {}", path, e))?;

    Ok(path)
}

fn convert_to_rgb24(
    frame: &DecodedFrame,
    src_fmt: Pixel,
    w: u32,
    h: u32,
) -> Result<Vec<u8>, String> {
    let dst_fmt = Pixel::RGB24;

    if src_fmt == dst_fmt {
        return Ok(frame.data.clone());
    }

    let mut src_frame = FfmpegFrame::new(src_fmt, w, h);
    fill_frame_data(&mut src_frame, &frame.data, src_fmt, w, h)?;

    let mut sws = SwsContext::get(src_fmt, w, h, dst_fmt, w, h, Flags::BILINEAR)
        .map_err(|e| format!("Failed to create swscale context: {}", e))?;

    let mut dst_frame = FfmpegFrame::new(dst_fmt, w, h);
    sws.run(&src_frame, &mut dst_frame)
        .map_err(|e| format!("swscale conversion failed: {}", e))?;

    let stride = dst_frame.stride(0);
    let row_bytes = (w * 3) as usize;
    let mut rgb_data = Vec::with_capacity(row_bytes * h as usize);
    let plane = dst_frame.data(0);
    for row in 0..h as usize {
        let start = row * stride;
        rgb_data.extend_from_slice(&plane[start..start + row_bytes]);
    }

    Ok(rgb_data)
}

fn fill_frame_data(
    dst: &mut FfmpegFrame,
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
                    "NV12 frame data too small: got {} bytes, need {}",
                    src_data.len(),
                    y_size + uv_size
                ));
            }

            let y_stride = dst.stride(0);
            let uv_stride = dst.stride(1);
            let y_plane = dst.data_mut(0);
            for row in 0..h as usize {
                let src_start = row * w as usize;
                let dst_start = row * y_stride;
                y_plane[dst_start..dst_start + w as usize]
                    .copy_from_slice(&src_data[src_start..src_start + w as usize]);
            }
            let uv_plane = dst.data_mut(1);
            for row in 0..(h / 2) as usize {
                let src_start = y_size + row * w as usize;
                let dst_start = row * uv_stride;
                uv_plane[dst_start..dst_start + w as usize]
                    .copy_from_slice(&src_data[src_start..src_start + w as usize]);
            }
        }
        Pixel::RGBA | Pixel::BGRA => {
            let stride = dst.stride(0);
            let row_bytes = (w * 4) as usize;
            if src_data.len() < row_bytes * h as usize {
                return Err(format!(
                    "RGBA/BGRA frame data too small: got {} bytes, need {}",
                    src_data.len(),
                    row_bytes * h as usize
                ));
            }
            let plane = dst.data_mut(0);
            for row in 0..h as usize {
                let src_start = row * row_bytes;
                let dst_start = row * stride;
                plane[dst_start..dst_start + row_bytes]
                    .copy_from_slice(&src_data[src_start..src_start + row_bytes]);
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
