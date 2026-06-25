use eframe::egui;

use crate::state::app_state::{ApiCommand, AppState};
use crate::theme::colors;

/// Draw the diagnostics panel for health, connectivity, and error logs
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Frame::NONE
        .fill(colors::GRAPHITE_SURFACE)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.colored_label(colors::RED_ACCENT, "Diagnostics");
            ui.separator();
            ui.add_space(8.0);

            // ---- Health Status ----
            section(ui, "Health");
            let health = &state.settings.health;
            status_row(ui, "Backend", health.backend);
            status_row(ui, "Auth", health.auth);
            status_row(ui, "SSE Stream", health.sse_connected);
            ui.colored_label(
                colors::TEXT_MUTED,
                format!("API URL: {}", health.api_base_url),
            );

            ui.add_space(8.0);

            // ---- Connection ----
            section(ui, "Connection");
            status_row(ui, "Logged In", state.is_logged_in);
            ui.colored_label(
                colors::TEXT_MUTED,
                format!("Server: {}", state.server_url),
            );
            if let Some(path) = &state.app_config_path {
                ui.colored_label(colors::TEXT_MUTED, format!("Config: {}", path));
            }

            ui.add_space(8.0);

            // ---- Task State ----
            section(ui, "Task State");
            if let Some(task) = &state.task.active_task {
                ui.colored_label(colors::TEXT_PRIMARY, format!("Status: {:?}", task.status));
                ui.colored_label(
                    colors::TEXT_MUTED,
                    format!("Elapsed: {}s", task.elapsed_seconds),
                );
                ui.colored_label(
                    colors::TEXT_MUTED,
                    format!("Timeline entries: {}", task.timeline.len()),
                );
            } else {
                ui.colored_label(colors::TEXT_MUTED, "No active task");
            }

            ui.add_space(8.0);

            // ---- Buttons ----
            ui.horizontal(|ui| {
                if ui.button("Refresh Health").clicked() {
                    state.api_commands.push(ApiCommand::RefreshHealth);
                    state.set_status("Refreshing health check...".to_string());
                }
                if ui.button("Test Connection").clicked() {
                    state.api_commands.push(ApiCommand::TestConnection);
                    state.set_status("Testing connection...".to_string());
                }
            });
        });
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.colored_label(colors::TEXT_SECONDARY, title);
    ui.separator();
}

fn status_row(ui: &mut egui::Ui, label: &str, ok: bool) {
    ui.horizontal(|ui| {
        let (color, symbol) = if ok {
            (colors::SUCCESS, "✓")
        } else {
            (colors::ERROR, "✗")
        };
        ui.colored_label(color, symbol);
        ui.colored_label(colors::TEXT_PRIMARY, label);
    });
}