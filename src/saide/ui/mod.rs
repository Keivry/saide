mod dialog;
mod indicator;
mod mapping;
mod player;
mod saide;
mod toolbar;

pub use {saide::SAideApp, toolbar::Toolbar};

#[derive(Default)]
pub struct VideoStats {
    pub fps: f32,
    pub total_frames: u32,
    pub dropped_frames: u32,
    pub latency_ms: f32,
}
