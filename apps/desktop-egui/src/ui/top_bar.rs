use eframe::egui::{self, Color32, Rounding};

use crate::state::app_state::AppState;
use crate::theme::colors;

/// Draw the top bar with session title, mode badge, health indicator, etc.
pub fn draw(ui: &mut egui::Ui, state: &AppState) {
    // Top bar with dark background
    egui::Frame::none()
        .fill(colors::GRAPHITE_ELEVATED)
        .rounding(Rounding {
            nw: 0.0,
            ne: 0.0,
            sw: 0.0,
            se: 0.0,
        })
        .inner_margin(egui::Margin::symmetric(12.0, 6.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Session title
                let title = state
                    .chat
                    .active_session()
                    .map(|s| &s.title[..])
                    .unwrap_or("Makima Agent");

                ui.heading(title);
                ui.add_space(12.0);

                // Mode badge
                if let Some(mode) = &state.settings.active_mode_slug {
                    let mode_name = state
                        .settings
                        .modes
                        .iter()
                        .find(|m| m.slug == *mode)
                        .map(|m| &m.name[..])
                        .unwrap_or(mode);

                    badge(ui, mode_name, colors::RED_DIM, colors::RED_ACCENT);
                }

                ui.add_space(8.0);

                // Model badge
                if state.settings.model_config.configured {
                    badge(
                        ui,
                        &state.settings.model_config.model,
                        colors::GRAPHITE_BORDER,
                        colors::TEXT_SECONDARY,
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Health indicator
                    let (health_color, health_text) = if state.settings.health.backend {
                        if state.is_logged_in {
                            (colors::SUCCESS, "Connected")
                        } else {
                            (colors::WARNING, "Unauthenticated")
                        }
                    } else {
                        (colors::ERROR, "Offline")
                    };

                    ui.colored_label(health_color, format!("● {}", health_text));

                    ui.add_space(4.0);

                    // SSE status
                    let sse_color = if state.settings.health.sse_connected {
                        colors::SUCCESS
                    } else {
                        colors::TEXT_MUTED
                    };
                    ui.colored_label(sse_color, "SSE");

                    // Session token estimate
                    if let Some(session) = state.chat.active_session() {
                        let tokens = session.estimated_token_count();
                        if tokens > 0 {
                            ui.add_space(12.0);
                            ui.colored_label(colors::TEXT_MUTED, format!("~{} tokens", tokens));
                        }
                    }
                });
            });
        });
}

fn badge(ui: &mut egui::Ui, text: &str, bg: Color32, fg: Color32) {
    egui::Frame::none()
        .fill(bg)
        .rounding(Rounding::same(4.0))
        .inner_margin(egui::Margin::symmetric(8.0, 2.0))
        .show(ui, |ui| {
            ui.colored_label(fg, text);
        });
}