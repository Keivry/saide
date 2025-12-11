mod v4l2_capture;
mod yuv_render;

// Re-export YuvRenderCallback but keep constructor internal
pub use {
    v4l2_capture::{V4l2Capture, Yu12Frame},
    yuv_render::{YuvRenderResources, new_yuv_render_callback},
};
