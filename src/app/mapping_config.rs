use {
    super::utils::{device_to_screen_coords, extract_position},
    crate::config::mapping::{AdbAction, Key},
    eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke},
    std::collections::HashMap,
};

/// Mapping configuration UI state
#[derive(Debug, Default)]
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

impl MappingConfigWindow {
    pub fn new() -> Self { Self::default() }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.reset_state();
        }
    }

    pub fn show(&mut self) { self.visible = true; }

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
        rotation: u32,
    ) -> MappingConfigEvent {
        let mut event = MappingConfigEvent::None;

        if !self.visible {
            return event;
        }

        // Draw semi-transparent overlay with interaction to consume events
        let _response = egui::Area::new(egui::Id::new("mapping_config_overlay"))
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
                        Pos2::new(
                            video_rect.left() + 20.0,
                            video_rect.top() + 50.0 + i as f32 * 20.0,
                        ),
                        egui::Align2::LEFT_TOP,
                        *text,
                        FontId::proportional(14.0),
                        Color32::LIGHT_GRAY,
                    );
                }

                // Draw existing mappings
                for (key, action) in mappings {
                    if let Some(device_pos) = extract_position(action)
                        && let Some(screen_pos) = device_to_screen_coords(
                            device_pos,
                            video_rect,
                            physical_size,
                            orientation,
                            capture_orientation,
                            rotation,
                        )
                    {
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

                // Handle clicks on the overlay
                if overlay_response.clicked() {
                    if let Some(pos) = overlay_response.interact_pointer_pos() {
                        event = MappingConfigEvent::RequestAddMapping(pos);
                    }
                } else if overlay_response.secondary_clicked()
                    && let Some(pos) = overlay_response.interact_pointer_pos()
                {
                    event = MappingConfigEvent::RequestDeleteMapping(pos);
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
}

impl MappingConfigWindow {
    /// Show key input dialog
    pub fn show_key_input_dialog(
        &mut self,
        ctx: &egui::Context,
        device_pos: (u32, u32),
    ) -> Option<Key> {
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
                                if let egui::Event::Key { key, pressed, .. } = event
                                    && *pressed
                                    && *key != egui::Key::Escape
                                {
                                    result = Some(*key);
                                    should_close = true;
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

    pub fn is_input_dialog_open(&self) -> bool { self.input_dialog_open }

    pub fn is_delete_dialog_open(&self) -> bool { self.delete_confirm_open }

    pub fn get_pending_input(&self) -> Option<(u32, u32)> { self.pending_input }

    pub fn get_delete_target(&self) -> Option<(Key, (u32, u32))> {
        if let Some(key) = self.delete_target_key
            && let Some(pos) = self.delete_target_pos
        {
            return Some((key, pos));
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
