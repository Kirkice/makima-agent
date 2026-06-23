//! Inspector Sidebar - 精简的上下文摘要
//!
//! 只展示当前最相关的上下文信息：
//! - Mode / Model
//! - Token / Cost
//! - Task 状态
//! - Voice / Avatar 状态

use eframe::egui;
use crate::state::app_state::AppState;
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    egui::ScrollArea::vertical()
        .id_salt("inspector_panel")
        .show(ui, |ui| {
            // ── Mode Section ─────────────────────────────────────
            section_header(ui, "Mode");
            if let Some(mode) = state.settings.active_mode() {
                ui.colored_label(colors::TEXT_PRIMARY, &mode.name);
                if let Some(desc) = &mode.description {
                    ui.colored_label(colors::TEXT_MUTED, desc);
                }
            } else {
                ui.colored_label(colors::TEXT_MUTED, "No mode selected");
            }
            ui.add_space(16.0);

            // ── Model Section ────────────────────────────────────
            section_header(ui, "Model");
            let model = &state.settings.model_config;
            if model.provider_configured {
                metric_row(ui, "Provider", &model.provider);
                metric_row(ui, "Model", &model.model);
                metric_row(ui, "Temperature", &format!("{:.2}", model.temperature));
            } else {
                ui.colored_label(colors::TEXT_MUTED, "Not configured");
            }
            ui.add_space(16.0);

            // ── Token / Cost Section ─────────────────────────────
            section_header(ui, "Session");
            if let Some(session) = state.chat.active_session() {
                let tokens = session.estimated_token_count();
                let cost = session.estimated_cost(state.settings.token_estimate_per_1k);
                metric_row(ui, "Tokens", &format!("{}", tokens));
                metric_row(ui, "Cost", &format!("${:.5}", cost));
            } else {
                ui.colored_label(colors::TEXT_MUTED, "No active session");
            }
            ui.add_space(16.0);

            // ── Task Status ──────────────────────────────────────
            section_header(ui, "Task");
            if let Some(task) = &state.task.active_task {
                let status_text = format!("{:?}", task.status);
                ui.colored_label(colors::TEXT_PRIMARY, status_text);
                metric_row(ui, "Elapsed", &format!("{}s", task.elapsed_seconds));
                if !task.timeline.is_empty() {
                    metric_row(ui, "Steps", &format!("{}", task.timeline.len()));
                }
            } else {
                ui.colored_label(colors::TEXT_MUTED, "No active task");
            }
            ui.add_space(16.0);

            // ── Voice Status ─────────────────────────────────────
            section_header(ui, "Voice");
            let vc = &state.voice_call;
            if vc.is_connected || vc.is_connecting {
                let (icon, color) = if vc.is_connected {
                    ("●", colors::SUCCESS)
                } else {
                    ("◌", colors::WARNING)
                };
                ui.horizontal(|ui| {
                    ui.colored_label(color, icon);
                    ui.colored_label(colors::TEXT_PRIMARY, &vc.status);
                });
                if vc.is_connected {
                    let mins = vc.call_duration_secs / 60;
                    let secs = vc.call_duration_secs % 60;
                    metric_row(ui, "Duration", &format!("{:02}:{:02}", mins, secs));
                }
            } else {
                metric_row(ui, "TTS", &state.settings.voice_config.tts_provider);
                if state.settings.voice_config.active_voice_id.is_some() {
                    metric_row(ui, "Voice", "Configured");
                }
            }
        });
}

fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.colored_label(colors::RED_ACCENT, title);
    ui.add_space(4.0);
}

fn metric_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(colors::TEXT_SECONDARY, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.colored_label(colors::TEXT_PRIMARY, value);
        });
    });
}