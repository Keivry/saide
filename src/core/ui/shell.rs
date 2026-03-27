// SPDX-License-Identifier: MIT OR Apache-2.0

use {
    super::{AppCommand, SAideApp},
    crate::config::ConfigManager,
    crossbeam_channel::Receiver,
    egui_event::{Dispatcher, EventRegistry},
};

pub struct AppShell {
    state: SAideApp,
    dispatcher: Dispatcher<SAideApp>,
    event_registry: EventRegistry,
}

impl AppShell {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        serial: &str,
        config_manager: ConfigManager,
        shutdown_rx: Receiver<()>,
        startup_error: Option<String>,
        startup_warnings: Vec<String>,
    ) -> Self {
        let state = SAideApp::new(
            cc,
            serial,
            config_manager,
            shutdown_rx,
            startup_error,
            startup_warnings,
        );
        let event_registry = EventRegistry::new();
        let mut dispatcher = Dispatcher::new();
        let _ = dispatcher.on::<AppCommand>(&event_registry, SAideApp::on_app_command);
        Self {
            state,
            dispatcher,
            event_registry,
        }
    }
}

impl eframe::App for AppShell {
    fn on_exit(&mut self) { self.state.on_exit_inner(); }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.event_registry.update();
        self.dispatcher
            .dispatch(&mut self.state, &self.event_registry);
        self.state.draw(ui, frame, &mut self.event_registry);
    }
}
