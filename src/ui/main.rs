use {
    crate::player::{new_yuv_render_callback, Yu12Frame, YuvRenderResources},
    eframe::egui::{self, Button, Color32, RichText},
    once_cell::sync::Lazy,
    std::{sync::Arc, time::Instant},
};

const BG_COLOR: Color32 = Color32::from_rgb(32, 32, 32);
const FG_COLOR: Color32 = Color32::from_rgb(220, 220, 220);

const TOOLBAR_WIDTH: f32 = 48.0;
const STATUSBAR_HEIGHT: f32 = 42.0;

const TOOLBAR_BTN_COUNT: usize = 1;
const TOOLBAR_BTN_SIZE: [f32; 2] = [42.0, 42.0];
const TOOLBAR_BTN_SPACING: f32 = 2.0;

struct ToolbarButton {
    lable: &'static str,
    tooltip: &'static str,
    callback: fn(&mut VideoApp, &egui::Context),
}

static TOOLBAR_BUTTONS: Lazy<Vec<ToolbarButton>> = Lazy::new(|| {
    vec![ToolbarButton {
        lable: "⟳",
        tooltip: "Rotate Video",
        callback: VideoApp::rotate,
    }]
});

/// Main application state
pub struct VideoApp {
    receiver: crossbeam_channel::Receiver<Arc<Yu12Frame>>,
    frame: Option<Arc<Yu12Frame>>,
    last_frame_instant: Option<Instant>,
    max_fps: f32,
    fps: f32,
    video_width: u32,
    video_height: u32,
    rotation: u32,
}

impl VideoApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        receiver: crossbeam_channel::Receiver<Arc<Yu12Frame>>,
        video_width: u32,
        video_height: u32,
        max_fps: f32,
    ) -> Self {
        if let Some(wgpu_state) = cc.wgpu_render_state.as_ref() {
            let resources = YuvRenderResources::new(&wgpu_state.device, wgpu_state.target_format);
            wgpu_state
                .renderer
                .write()
                .callback_resources
                .insert(resources);
        }

        Self {
            receiver,
            frame: None,
            last_frame_instant: None,
            max_fps,
            fps: 0.0,
            video_width,
            video_height,
            rotation: 0,
        }
    }

    pub const fn toolbar_width() -> f32 { TOOLBAR_WIDTH }

    pub const fn statusbar_height() -> f32 { STATUSBAR_HEIGHT }

    fn effective_dimensions(&self) -> (u32, u32) {
        if self.rotation & 1 == 0 {
            (self.video_width, self.video_height)
        } else {
            (self.video_height, self.video_width)
        }
    }

    /// Resize the application window to match video dimensions
    fn resize(&mut self, ctx: &egui::Context) {
        let (w, h) = self.effective_dimensions();
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            w as f32 + TOOLBAR_WIDTH,
            h as f32 + STATUSBAR_HEIGHT,
        )));
    }

    /// Rotate video by 90 degrees clockwise
    fn rotate(&mut self, ctx: &egui::Context) {
        self.rotation = (self.rotation + 1) % 4;
        self.resize(ctx);
    }

    /// Draw the toolbar on the left side
    fn draw_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = TOOLBAR_BTN_SPACING;

            // Center buttons vertically
            let rect = ui.available_rect_before_wrap();
            let desired_height = (TOOLBAR_BTN_SIZE[1] + TOOLBAR_BTN_SPACING)
                * TOOLBAR_BTN_COUNT as f32
                + TOOLBAR_BTN_SPACING;
            let top_padding = (rect.height() - desired_height) / 2.0;
            ui.add_space(top_padding);

            ui.add_space(TOOLBAR_BTN_SPACING);
            for btn in TOOLBAR_BUTTONS.iter() {
                if ui
                    .add_sized(
                        TOOLBAR_BTN_SIZE,
                        Button::new(RichText::new(btn.lable).color(FG_COLOR).size(16.0)),
                    )
                    .on_hover_text(btn.tooltip)
                    .clicked()
                {
                    (btn.callback)(self, ui.ctx());
                }
                ui.add_space(TOOLBAR_BTN_SPACING);
            }
        });
    }

    /// Draw the status bar at the top
    fn draw_statusbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(format!(
                "Resolution: {}x{} | FPS: {} |Rotation: {}°",
                self.video_width,
                self.video_height,
                self.fps.min(self.max_fps) as u32,
                self.rotation * 90
            ));
        });
    }

    /// Draw the main v4l2 player area
    fn draw_v4l2_player(&mut self, ui: &mut egui::Ui) {
        // Receive latest frame
        while let Ok(frame) = self.receiver.try_recv() {
            self.frame = Some(frame);
        }
        // Update FPS
        if let Some(last_instant) = self.last_frame_instant {
            let delta = Instant::now().duration_since(last_instant).as_secs_f32();
            if delta > 0.0 {
                self.fps = 0.95 * self.fps + 0.05 * (1.0 / delta);
            }
        }

        self.last_frame_instant = Some(Instant::now());

        // Always maintain aspect ratio
        let (eff_w, eff_h) = self.effective_dimensions();
        let aspect = eff_w as f32 / eff_h as f32;

        let rect = ui.available_size();
        let (width, height) = if rect.x / rect.y > aspect {
            (rect.y * aspect, rect.y)
        } else {
            (rect.x, rect.x / aspect)
        };
        let rect =
            egui::Rect::from_center_size(ui.max_rect().center(), eframe::egui::vec2(width, height));
        let _ = ui.allocate_rect(rect, eframe::egui::Sense::hover());

        // Draw video frame or placeholder
        if let Some(frame) = &self.frame {
            let callback = eframe::egui_wgpu::Callback::new_paint_callback(
                rect,
                new_yuv_render_callback(Arc::clone(frame), self.rotation),
            );
            ui.painter().add(callback);
        } else {
            ui.painter()
                .rect_filled(rect, 0.0, eframe::egui::Color32::from_gray(32));
            ui.painter().text(
                rect.center(),
                eframe::egui::Align2::CENTER_CENTER,
                "Waiting for video...",
                eframe::egui::FontId::proportional(24.0),
                eframe::egui::Color32::GRAY,
            );
        }
    }
}

impl eframe::App for VideoApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        eframe::egui::SidePanel::left("Toolbar")
            .frame(eframe::egui::Frame::NONE.fill(BG_COLOR))
            .resizable(false)
            .exact_width(TOOLBAR_WIDTH)
            .show(ctx, |ui| {
                self.draw_toolbar(ui);
            });

        eframe::egui::TopBottomPanel::top("Status Bar")
            .frame(eframe::egui::Frame::NONE.fill(eframe::egui::Color32::from_gray(50)))
            .resizable(false)
            .exact_height(STATUSBAR_HEIGHT)
            .show(ctx, |ui| {
                self.draw_statusbar(ui);
            });

        eframe::egui::CentralPanel::default()
            .frame(eframe::egui::Frame::NONE)
            .show(ctx, |ui| {
                self.draw_v4l2_player(ui);
            });

        ctx.request_repaint();
    }
}
