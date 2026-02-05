mod editor;
mod indicator;
mod main;
mod player;
mod theme;
mod toolbar;

pub use main::SAideApp;

#[derive(Default)]
pub struct VideoStats {
    pub fps: f32,
    pub total_frames: u32,
    pub dropped_frames: u32,
    pub latency_ms: f32,
    pub latency_decode_ms: f32,
    pub latency_upload_ms: f32,
    pub latency_p95_ms: f32,
}
