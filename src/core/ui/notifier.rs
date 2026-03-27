// SPDX-License-Identifier: MIT OR Apache-2.0

use {
    egui::{Color32, CornerRadius, FontId, Id, Margin, RichText},
    std::{
        cell::RefCell,
        collections::VecDeque,
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    },
};

const TOAST_DURATION: Duration = Duration::from_secs(3);
const TOAST_FADE_DURATION: Duration = Duration::from_millis(300);
const TOAST_MAX_VISIBLE: usize = 5;
const TOAST_WIDTH: f32 = 320.0;
const TOAST_PADDING_H: i8 = 12;
const TOAST_PADDING_V: i8 = 8;
const TOAST_MARGIN: f32 = 8.0;
const TOAST_DEFAULT_HEIGHT: f32 = 36.0;

static TOAST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct Toast {
    id: u64,
    message: String,
    created_at: Instant,
}

impl Toast {
    fn new(message: impl Into<String>) -> Self {
        Self {
            id: TOAST_COUNTER.fetch_add(1, Ordering::Relaxed),
            message: message.into(),
            created_at: Instant::now(),
        }
    }

    fn age(&self) -> Duration { self.created_at.elapsed() }

    fn opacity(&self) -> f32 {
        let age = self.age();
        if age < TOAST_FADE_DURATION {
            age.as_secs_f32() / TOAST_FADE_DURATION.as_secs_f32()
        } else if age > TOAST_DURATION - TOAST_FADE_DURATION {
            let remaining = TOAST_DURATION.saturating_sub(age);
            remaining.as_secs_f32() / TOAST_FADE_DURATION.as_secs_f32()
        } else {
            1.0
        }
    }

    fn is_expired(&self) -> bool { self.age() >= TOAST_DURATION }
}

pub struct Notifier {
    toasts: RefCell<VecDeque<Toast>>,
}

impl Notifier {
    pub fn new() -> Self {
        Self {
            toasts: RefCell::new(VecDeque::new()),
        }
    }

    pub fn notify(&self, message: &str) {
        let mut toasts = self.toasts.borrow_mut();
        if toasts.len() >= TOAST_MAX_VISIBLE {
            toasts.pop_front();
        }
        toasts.push_back(Toast::new(message));
    }

    pub fn draw(&self, ui: &egui::Ui, screen_rect: egui::Rect) {
        let mut toasts = self.toasts.borrow_mut();
        toasts.retain(|t| !t.is_expired());

        if toasts.is_empty() {
            return;
        }

        let anchor_x = screen_rect.right() - TOAST_MARGIN;
        let mut anchor_y = screen_rect.top() + TOAST_MARGIN;

        for toast in toasts.iter() {
            let opacity = toast.opacity().clamp(0.0, 1.0);
            let alpha = (opacity * 220.0) as u8;

            let response = egui::Area::new(Id::new("toast").with(toast.id))
                .fixed_pos(egui::pos2(anchor_x - TOAST_WIDTH, anchor_y))
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(30, 30, 30, alpha))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(TOAST_PADDING_H, TOAST_PADDING_V))
                        .show(ui, |ui| {
                            ui.set_width(TOAST_WIDTH - TOAST_PADDING_H as f32 * 2.0);
                            ui.label(
                                RichText::new(&toast.message)
                                    .font(FontId::proportional(13.0))
                                    .color(Color32::from_rgba_unmultiplied(240, 240, 240, alpha)),
                            );
                        });
                });

            // Use the actual rendered height; fall back to a default on first frame
            // (egui reports zero height before the first layout pass completes).
            let height = response.response.rect.height();
            let effective_height = if height > 0.0 {
                height
            } else {
                TOAST_DEFAULT_HEIGHT
            };
            anchor_y += effective_height + TOAST_MARGIN;
        }

        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}
