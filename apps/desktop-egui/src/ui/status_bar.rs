use eframe::egui::{self, Frame, Margin};

use crate::state::app_state::AppState;
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, state: &AppState) {
    Frame::NONE
        .fill(colors::SURFACE)
        .inner_margin(Margin::symmetric(12, 4))
        .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(task) = &state.task.active_task {
                    ui.colored_label(colors::TEXT_MUTED, format!("Task: {:?}", task.status));
                    ui.add_space(12.0);
                }

                let total_tokens = state.total_token_usage();
                let total_cost = state.total_estimated_cost();
                if total_tokens > 0 {
                    ui.colored_label(
                        colors::TEXT_MUTED,
                        format!("~{} tokens | ${:.5}", total_tokens, total_cost),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let login_text = if state.is_logged_in {
                        "Connected"
                    } else {
                        "Disconnected"
                    };
                    let login_color = if state.is_logged_in {
                        colors::SUCCESS
                    } else {
                        colors::WARNING
                    };

                    let network_text = if state.settings.health.backend {
                        "Online"
                    } else {
                        "Offline"
                    };
                    let network_color = if state.settings.health.backend {
                        colors::SUCCESS
                    } else {
                        colors::ERROR
                    };

                    ui.colored_label(login_color, login_text);
                    ui.add_space(10.0);
                    ui.colored_label(network_color, network_text);
                });
            });
        });
}
