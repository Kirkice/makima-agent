use eframe::egui::{self, CornerRadius, Frame, Margin, Stroke};

use crate::app::LoginDialogState;
use crate::config::app_config::DEFAULT_SERVER_URL;
use crate::state::app_state::AppState;
use crate::theme::colors;

const CARD_WIDTH: f32 = 460.0;
const CARD_HEIGHT: f32 = 360.0;

/// Draw the login screen. Returns true when login button is clicked.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState, login_state: &mut LoginDialogState) -> bool {
    let mut clicked = false;
    let full_rect = ui.max_rect();
    let painter = ui.painter();

    painter.rect_filled(full_rect, 0.0, colors::BG);

    let glow_center = egui::pos2(full_rect.center().x, full_rect.center().y - 42.0);
    painter.circle_filled(
        glow_center,
        280.0,
        egui::Color32::from_rgba_unmultiplied(120, 54, 58, 24),
    );

    let card_rect = egui::Rect::from_center_size(
        glow_center + egui::vec2(0.0, 30.0),
        egui::vec2(CARD_WIDTH, CARD_HEIGHT),
    );

    ui.allocate_ui_at_rect(card_rect, |ui| {
        Frame::NONE
            .fill(colors::GRAPHITE_ELEVATED)
            .stroke(Stroke::new(1.0, colors::GRAPHITE_BORDER))
            .corner_radius(CornerRadius::same(20))
            .inner_margin(Margin::symmetric(32, 28))
            .show(ui, |ui| {
                ui.set_min_size(egui::vec2(CARD_WIDTH - 64.0, CARD_HEIGHT - 56.0));

                ui.vertical_centered(|ui| {
                    ui.colored_label(
                        colors::TEXT_PRIMARY,
                        egui::RichText::new("Makima Agent").size(30.0).strong(),
                    );
                    ui.add_space(6.0);
                    ui.colored_label(
                        colors::TEXT_MUTED,
                        "Connect to your backend and start a focused workspace.",
                    );
                });

                ui.add_space(28.0);

                field(ui, "Server URL", &mut login_state.server_url, DEFAULT_SERVER_URL);
                ui.add_space(12.0);
                field(ui, "Username", &mut login_state.username, "admin");
                ui.add_space(12.0);
                password_field(ui, "Password", &mut login_state.password, "Enter password");

                if let Some(error) = &login_state.error {
                    ui.add_space(12.0);
                    ui.colored_label(colors::ERROR, error);
                }

                ui.add_space(22.0);

                let can_login = !login_state.username.is_empty()
                    && !login_state.password.is_empty()
                    && !login_state.server_url.is_empty()
                    && !login_state.loading;
                let btn_text = if login_state.loading {
                    "Connecting..."
                } else {
                    "Connect"
                };

                ui.vertical_centered(|ui| {
                    if ui
                        .add_enabled(
                            can_login,
                            egui::Button::new(btn_text).min_size(egui::vec2(200.0, 40.0)),
                        )
                        .clicked()
                    {
                        login_state.loading = true;
                        login_state.error = None;
                        state.server_url = login_state.server_url.clone();
                        state.set_status("Connecting...".to_string());
                        clicked = true;
                    }
                });

                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    if let Some(status) = &state.status_message {
                        let color = if login_state.loading {
                            colors::TEXT_MUTED
                        } else if login_state.error.is_some() {
                            colors::ERROR
                        } else {
                            colors::TEXT_MUTED
                        };
                        ui.colored_label(color, egui::RichText::new(status).size(12.0));
                        ui.add_space(6.0);
                    }
                    ui.colored_label(
                        colors::TEXT_MUTED,
                        egui::RichText::new("Make sure your Makima backend is running.").size(12.0),
                    );
                });
            });
    });

    clicked
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    ui.colored_label(colors::TEXT_SECONDARY, label);
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(egui::RichText::new(hint).color(colors::TEXT_MUTED))
            .desired_width(f32::INFINITY),
    );
}

fn password_field(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    ui.colored_label(colors::TEXT_SECONDARY, label);
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::singleline(value)
            .password(true)
            .hint_text(egui::RichText::new(hint).color(colors::TEXT_MUTED))
            .desired_width(f32::INFINITY),
    );
}
