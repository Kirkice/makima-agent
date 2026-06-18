use eframe::egui::{self, Rounding};

use crate::app::LoginDialogState;
use crate::state::app_state::AppState;
use crate::theme::colors;

/// Draw the login screen (shown when user is not authenticated)
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, login_state: &mut LoginDialogState) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.2);

        egui::Frame::none()
            .fill(colors::GRAPHITE_ELEVATED)
            .stroke(egui::Stroke::new(1.0, colors::GRAPHITE_BORDER))
            .rounding(Rounding::same(12.0))
            .inner_margin(egui::Margin::symmetric(32.0, 24.0))
            .show(ui, |ui| {
                ui.set_max_width(400.0);

                ui.heading("Makima Agent");
                ui.colored_label(colors::TEXT_MUTED, "Connect to your backend server");
                ui.add_space(16.0);

                // Server URL
                ui.colored_label(colors::TEXT_SECONDARY, "Server URL");
                ui.add(
                    egui::TextEdit::singleline(&mut login_state.server_url)
                        .hint_text("http://localhost:8000")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);

                // Username
                ui.colored_label(colors::TEXT_SECONDARY, "Username");
                ui.add(
                    egui::TextEdit::singleline(&mut login_state.username)
                        .hint_text("admin")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);

                // Password
                ui.colored_label(colors::TEXT_SECONDARY, "Password");
                ui.add(
                    egui::TextEdit::singleline(&mut login_state.password)
                        .password(true)
                        .hint_text("••••••••")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(16.0);

                // Error
                if let Some(error) = &login_state.error {
                    ui.colored_label(colors::ERROR, error);
                    ui.add_space(8.0);
                }

                // Login button
                let can_login = !login_state.username.is_empty()
                    && !login_state.password.is_empty()
                    && !login_state.server_url.is_empty()
                    && !login_state.loading;

                let button_text = if login_state.loading { "Connecting..." } else { "Connect" };

                if ui.add_enabled(can_login, egui::Button::new(button_text)).clicked() {
                    login_state.loading = true;
                    login_state.error = None;
                    state.server_url = login_state.server_url.clone();
                    state.set_status("Connecting...".to_string());
                }

                ui.add_space(8.0);
                ui.colored_label(colors::TEXT_MUTED, "Make sure your Makima backend is running.");
            });
    });
}

/// Draw the status bar at the bottom of the window
pub fn draw_status_bar(ui: &mut egui::Ui, state: &AppState) {
    egui::Frame::none()
        .fill(colors::GRAPHITE_ELEVATED)
        .inner_margin(egui::Margin::symmetric(12.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let auth_color = if state.is_logged_in { colors::SUCCESS } else { colors::WARNING };
                let auth_text = if state.is_logged_in { "Authenticated" } else { "Not logged in" };
                ui.colored_label(auth_color, format!("● {}", auth_text));

                ui.add_space(12.0);

                let sse_color = if state.settings.health.sse_connected { colors::SUCCESS } else { colors::TEXT_MUTED };
                ui.colored_label(sse_color, "SSE");

                ui.add_space(12.0);

                if let Some(task) = &state.task.active_task {
                    ui.colored_label(colors::SUCCESS, format!("Task: {}", task.status.label()));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(msg) = &state.status_message {
                        ui.colored_label(colors::TEXT_MUTED, msg);
                    }

                    let total_cost = state.total_estimated_cost();
                    if total_cost > 0.0 {
                        ui.colored_label(colors::TEXT_MUTED, format!("Total: ${:.5}", total_cost));
                    }

                    if state.settings.voice_config.tts_provider != "none" {
                        ui.colored_label(colors::TEXT_MUTED, "🎤");
                    }
                });
            });
        });
}