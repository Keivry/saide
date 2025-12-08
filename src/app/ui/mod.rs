mod dialog;
mod main;
mod mapping;
mod player;
mod status_bar;
mod toolbar;

pub use main::SAideApp;

#[derive(Default)]
pub struct VideoStats {
    pub fps: f32,
    pub total_frames: u32,
    pub dropped_frames: u32,
    pub latency_ms: f32,
}
