use eframe::egui::{self, Frame, Margin};

use crate::state::app_state::AppState;
use crate::theme::colors;

/// Rich status bar (VSCode-style) that shows session & agent context at a glance.
/// Information was migrated from the old inspector.rs "Context" panel.
pub fn draw(ui: &mut egui::Ui, state: &AppState) {
    Frame::NONE
        .fill(colors::SURFACE)
        .inner_margin(Margin::symmetric(12, 4))
        .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // --- Session info (from inspector.rs) ---
                if let Some(session) = state.chat.active_session() {
                    ui.label(
                        egui::RichText::new(format!("📨 {} msgs", session.messages.len()))
                            .size(12.0)
                            .color(colors::TEXT_MUTED),
                    );
                    ui.add_space(8.0);

                    let tokens = session.estimated_token_count();
                    if tokens > 0 {
                        ui.label(
                            egui::RichText::new(format!("🔢 {} tokens", format_tokens(tokens)))
                                .size(12.0)
                                .color(colors::TEXT_MUTED),
                        );
                        ui.add_space(8.0);

                        let cost = session.estimated_cost(state.settings.token_estimate_per_1k);
                        ui.label(
                            egui::RichText::new(format!("💰 ${:.5}", cost))
                                .size(12.0)
                                .color(colors::TEXT_MUTED),
                        );
                        ui.add_space(8.0);
                    }
                }

                // --- Agent info (from inspector.rs) ---
                let mode_name = state
                    .settings
                    .active_mode()
                    .map(|m| compact_emoji_name(&m.name))
                    .unwrap_or_else(|| "No mode".to_string());
                ui.label(
                    egui::RichText::new(format!("🎯 {}", mode_name))
                        .size(12.0)
                        .color(colors::RED_ACCENT),
                );
                ui.add_space(8.0);

                let model_name = if state.settings.model_config.configured {
                    &state.settings.model_config.model
                } else {
                    "No model"
                };
                ui.label(
                    egui::RichText::new(format!("🤖 {}", model_name))
                        .size(12.0)
                        .color(colors::INFO),
                );
                ui.add_space(8.0);

                // --- Task status (from inspector.rs) ---
                if let Some(task) = &state.task.active_task {
                    let (label, color) = match task.status {
                        crate::state::task_state::TaskStatus::Running => ("⚡ Running".to_string(), colors::SUCCESS),
                        crate::state::task_state::TaskStatus::Idle => ("Idle".to_string(), colors::TEXT_MUTED),
                        _ => (format!("✅ {} steps", task.timeline.len()), colors::INFO),
                    };
                    ui.label(
                        egui::RichText::new(label)
                            .size(12.0)
                            .color(color),
                    );
                    ui.add_space(8.0);
                }

                // --- Voice status (from inspector.rs) ---
                let (voice_label, voice_color) = if state.voice_call.is_connected {
                    ("🎤 Connected", colors::SUCCESS)
                } else if state.voice_call.is_connecting {
                    ("🎤 Connecting", colors::WARNING)
                } else {
                    ("🎤 Idle", colors::TEXT_MUTED)
                };
                ui.label(
                    egui::RichText::new(voice_label)
                        .size(12.0)
                        .color(voice_color),
                );

                // --- Right side: connection status ---
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let login_text = if state.is_logged_in { "Connected" } else { "Disconnected" };
                    let login_color = if state.is_logged_in { colors::SUCCESS } else { colors::WARNING };
                    let network_text = if state.settings.health.backend { "Online" } else { "Offline" };
                    let network_color = if state.settings.health.backend { colors::SUCCESS } else { colors::ERROR };

                    ui.label(
                        egui::RichText::new(login_text)
                            .size(12.0)
                            .color(login_color),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(network_text)
                            .size(12.0)
                            .color(network_color),
                    );
                });
            });
        });
}

fn compact_emoji_name(name: &str) -> String {
    if let Some(idx) = name.find(|c: char| c.is_alphabetic()) {
        if idx > 0 {
            return format!("{}{}", name[..idx].trim_end(), &name[idx..]);
        }
    }
    name.to_string()
}

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1000 {
        format!("{:.1}K", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}