use {
    crate::config::mapping::{AdbAction, Key},
    eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke},
    std::collections::HashMap,
};

/// Mapping configuration UI state
pub struct MappingConfigWindow {
    /// Whether the config window is visible
    pub visible: bool,

    /// Pending key input position (device coordinates)
    pending_input: Option<(u32, u32)>,

    /// Input dialog state
    input_dialog_open: bool,
    input_key_text: String,

    /// Delete confirmation dialog state
    delete_confirm_open: bool,
    delete_target_key: Option<Key>,
    delete_target_pos: Option<(u32, u32)>,
}

impl Default for MappingConfigWindow {
    fn default() -> Self {
        Self {
            visible: false,
            pending_input: None,
            input_dialog_open: false,
            input_key_text: String::new(),
            delete_confirm_open: false,
            delete_target_key: None,
            delete_target_pos: None,
        }
    }
}

impl MappingConfigWindow {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.reset_state();
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.reset_state();
    }

    fn reset_state(&mut self) {
        self.pending_input = None;
        self.input_dialog_open = false;
        self.input_key_text.clear();
        self.delete_confirm_open = false;
        self.delete_target_key = None;
        self.delete_target_pos = None;
    }

    /// Draw the mapping configuration overlay
    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        video_rect: Rect,
        mappings: &HashMap<Key, AdbAction>,
        physical_size: (u32, u32),
        orientation: u32,
        capture_orientation: u32,
    ) -> MappingConfigEvent {
        let mut event = MappingConfigEvent::None;

        if !self.visible {
            return event;
        }

        // Draw semi-transparent overlay with interaction to consume events
        let response = egui::Area::new(egui::Id::new("mapping_config_overlay"))
            .fixed_pos(video_rect.min)
            .interactable(true)
            .show(ctx, |ui| {
                // Create an invisible button covering the entire video rect to capture clicks
                let overlay_response = ui.allocate_rect(video_rect, egui::Sense::click());
                
                let painter = ui.painter();

                // Draw semi-transparent background
                painter.rect_filled(
                    video_rect,
                    0.0,
                    Color32::from_rgba_premultiplied(0, 0, 0, 180),
                );

                // Draw title
                painter.text(
                    Pos2::new(video_rect.center().x, video_rect.top() + 20.0),
                    egui::Align2::CENTER_TOP,
                    "Mapping Configuration Mode",
                    FontId::proportional(20.0),
                    Color32::WHITE,
                );

                // Draw instructions
                let instructions = [
                    "Left Click: Add key mapping",
                    "Right Click: Delete nearest mapping",
                    "Press ESC to exit",
                ];
                for (i, text) in instructions.iter().enumerate() {
                    painter.text(
                        Pos2::new(video_rect.left() + 20.0, video_rect.top() + 50.0 + i as f32 * 20.0),
                        egui::Align2::LEFT_TOP,
                        *text,
                        FontId::proportional(14.0),
                        Color32::LIGHT_GRAY,
                    );
                }

                // Draw existing mappings
                for (key, action) in mappings {
                    if let Some(device_pos) = Self::extract_position(action) {
                        if let Some(screen_pos) = device_to_screen_coords(
                            device_pos,
                            video_rect,
                            physical_size,
                            orientation,
                            capture_orientation,
                        ) {
                            // Draw circle marker
                            painter.circle_filled(screen_pos, 8.0, Color32::from_rgb(100, 200, 255));
                            painter.circle_stroke(screen_pos, 8.0, Stroke::new(2.0, Color32::WHITE));

                            // Draw key label
                            let key_text = format!("{:?}", key);
                            painter.text(
                                Pos2::new(screen_pos.x, screen_pos.y - 15.0),
                                egui::Align2::CENTER_BOTTOM,
                                &key_text,
                                FontId::proportional(12.0),
                                Color32::WHITE,
                            );
                        }
                    }
                }

                // Handle clicks on the overlay
                if overlay_response.clicked() {
                    if let Some(pos) = overlay_response.interact_pointer_pos() {
                        event = MappingConfigEvent::RequestAddMapping(pos);
                    }
                } else if overlay_response.secondary_clicked() {
                    if let Some(pos) = overlay_response.interact_pointer_pos() {
                        event = MappingConfigEvent::RequestDeleteMapping(pos);
                    }
                }
                
                // Check for ESC key to exit
                ui.input(|input| {
                    if input.key_pressed(egui::Key::Escape) {
                        event = MappingConfigEvent::Close;
                    }
                });
            });

        event
    }

    /// Extract position from AdbAction
    fn extract_position(action: &AdbAction) -> Option<(u32, u32)> {
        match action {
            AdbAction::Tap { x, y } => Some((*x, *y)),
            AdbAction::TouchDown { x, y } => Some((*x, *y)),
            _ => None,
        }
    }

    /// Show key input dialog
    pub fn show_key_input_dialog(&mut self, ctx: &egui::Context, device_pos: (u32, u32)) -> Option<Key> {
        if !self.input_dialog_open {
            return None;
        }

        let mut result = None;
        let mut should_close = false;

        egui::Window::new("Add Key Mapping")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("Position: ({}, {})", device_pos.0, device_pos.1));
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Press a key:");
                    let response = ui.text_edit_singleline(&mut self.input_key_text);
                    
                    if response.has_focus() {
                        ui.input(|input| {
                            for event in &input.events {
                                if let egui::Event::Key { key, pressed, .. } = event {
                                    if *pressed && *key != egui::Key::Escape {
                                        result = Some(*key);
                                        should_close = true;
                                    }
                                }
                            }
                        });
                    }
                    
                    if !response.has_focus() {
                        response.request_focus();
                    }
                });

                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        should_close = true;
                    }
                });
            });

        if should_close {
            self.input_dialog_open = false;
            self.input_key_text.clear();
            self.pending_input = None;
        }

        result
    }

    /// Show delete confirmation dialog
    pub fn show_delete_confirm_dialog(
        &mut self,
        ctx: &egui::Context,
        nearest_key: Key,
        nearest_pos: (u32, u32),
    ) -> Option<bool> {
        if !self.delete_confirm_open {
            return None;
        }

        let mut result = None;
        let mut should_close = false;

        egui::Window::new("Delete Mapping")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "Delete mapping for key {:?} at ({}, {})?",
                    nearest_key, nearest_pos.0, nearest_pos.1
                ));
                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Confirm").clicked() {
                        result = Some(true);
                        should_close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        result = Some(false);
                        should_close = true;
                    }
                });
            });

        if should_close {
            self.delete_confirm_open = false;
            self.delete_target_key = None;
            self.delete_target_pos = None;
        }

        result
    }

    pub fn is_input_dialog_open(&self) -> bool {
        self.input_dialog_open
    }

    pub fn is_delete_dialog_open(&self) -> bool {
        self.delete_confirm_open
    }

    pub fn get_pending_input(&self) -> Option<(u32, u32)> {
        self.pending_input
    }

    pub fn get_delete_target(&self) -> Option<(Key, (u32, u32))> {
        if let Some(key) = self.delete_target_key {
            if let Some(pos) = self.delete_target_pos {
                return Some((key, pos));
            }
        }
        None
    }

    /// Request to show input dialog
    pub fn request_input_dialog(&mut self, device_pos: (u32, u32)) {
        self.pending_input = Some(device_pos);
        self.input_dialog_open = true;
    }

    /// Request to show delete dialog
    pub fn request_delete_dialog(&mut self, key: Key, pos: (u32, u32)) {
        self.delete_target_key = Some(key);
        self.delete_target_pos = Some(pos);
        self.delete_confirm_open = true;
    }
}

/// Events generated by the mapping config window
#[derive(Debug, Clone)]
pub enum MappingConfigEvent {
    None,
    Close,
    RequestAddMapping(Pos2),
    RequestDeleteMapping(Pos2),
}

/// Convert device coordinates to screen coordinates in video rect
fn device_to_screen_coords(
    device_pos: (u32, u32),
    video_rect: Rect,
    physical_size: (u32, u32),
    orientation: u32,
    capture_orientation: u32,
) -> Option<Pos2> {
    // This is the inverse of coordinate_transform in main.rs
    // 
    // coordinate_transform does:
    // 1. Screen -> Video coords (inverse user rotation)
    // 2. Video -> Device coords (apply total_rotation)
    //
    // We need to do:
    // 1. Device -> Video coords (inverse total_rotation)
    // 2. Video -> Screen coords (apply user rotation)
    
    let total_rotation = (capture_orientation + orientation) % 4;
    
    // Calculate device logical size at current orientation
    let (device_w, device_h) = if orientation & 1 == 0 {
        (physical_size.0 as f32, physical_size.1 as f32)
    } else {
        (physical_size.1 as f32, physical_size.0 as f32)
    };
    
    let video_width = video_rect.width();
    let video_height = video_rect.height();
    
    // Note: video_rect dimensions are after user rotation
    // We need to determine original video dimensions
    // Assuming rotation is 0 in config mode, video dimensions stay the same
    let video_w = video_width;
    let video_h = video_height;
    
    let device_x = device_pos.0 as f32;
    let device_y = device_pos.1 as f32;
    
    // Step 1: Inverse total rotation to get video coordinates
    let (video_x, video_y) = match total_rotation {
        // 0 degrees
        0 => {
            let scale_x = video_w / device_w;
            let scale_y = video_h / device_h;
            (device_x * scale_x, device_y * scale_y)
        }
        // 90 degrees clockwise - inverse transform
        1 => {
            let scale_x = video_h / device_w;
            let scale_y = video_w / device_h;
            ((device_h - device_y) * scale_y, device_x * scale_x)
        }
        // 180 degrees
        2 => {
            let scale_x = video_w / device_w;
            let scale_y = video_h / device_h;
            ((device_w - device_x) * scale_x, (device_h - device_y) * scale_y)
        }
        // 270 degrees clockwise
        3 => {
            let scale_x = video_h / device_w;
            let scale_y = video_w / device_h;
            (device_y * scale_y, (device_w - device_x) * scale_x)
        }
        _ => return None,
    };
    
    // Step 2: Apply user rotation (assuming rotation is 0 in config mode)
    // For now we assume no user rotation during config
    let rel_x = video_x;
    let rel_y = video_y;
    
    // Convert to screen coordinates
    let screen_x = video_rect.left() + rel_x;
    let screen_y = video_rect.top() + rel_y;
    
    Some(Pos2::new(screen_x, screen_y))
}
