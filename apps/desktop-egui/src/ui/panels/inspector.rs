use eframe::egui::{self, CornerRadius};

use crate::state::app_state::AppState;
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.vertical(|ui| {
        header(ui);
        ui.add_space(14.0);

        info_group(
            ui,
            "Mode",
            state
                .settings
                .active_mode()
                .map(|mode| mode.name.as_str())
                .unwrap_or("No mode selected"),
        );

        info_group(
            ui,
            "Model",
            if state.settings.model_config.configured {
                &state.settings.model_config.model
            } else {
                "Not configured"
            },
        );

        if let Some(session) = state.chat.active_session() {
            info_group(ui, "Session", &format!("{} messages", session.messages.len()));
            info_group(
                ui,
                "Usage",
                &format!(
                    "{} tok · ${:.5}",
                    session.estimated_token_count(),
                    session.estimated_cost(state.settings.token_estimate_per_1k)
                ),
            );
        }

        if let Some(task) = &state.task.active_task {
            info_group(ui, "Task", &format!("{:?}", task.status));
            info_group(ui, "Timeline", &format!("{} steps", task.timeline.len()));
        } else {
            info_group(ui, "Task", "Idle");
        }

        let voice_status = if state.voice_call.is_connected {
            "Connected"
        } else if state.voice_call.is_connecting {
            "Connecting"
        } else {
            "Idle"
        };
        info_group(ui, "Voice", voice_status);
    });
}

fn header(ui: &mut egui::Ui) {
    ui.colored_label(
        colors::TEXT_PRIMARY,
        egui::RichText::new("Context").size(15.0).strong(),
    );
    ui.colored_label(colors::TEXT_MUTED, "Current session summary");
}

fn info_group(ui: &mut egui::Ui, label: &str, value: &str) {
    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.colored_label(colors::TEXT_MUTED, label);
            ui.colored_label(
                colors::TEXT_PRIMARY,
                egui::RichText::new(value).size(13.0).strong(),
            );
        });
    ui.add_space(8.0);
}
