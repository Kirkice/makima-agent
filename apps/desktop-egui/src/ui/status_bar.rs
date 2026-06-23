//! Status Bar - 底部状态栏 (24px)
//!
//! 显示连接状态、token 用量、当前任务状态等轻量信息

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
                // Connection status
                let (icon, color, text) = if state.settings.health.backend && state.is_logged_in {
                    ("●", colors::SUCCESS, "Connected")
                } else if state.settings.health.backend {
                    ("●", colors::WARNING, "Not logged in")
                } else {
                    ("●", colors::ERROR, "Offline")
                };
                ui.colored_label(color, icon);
                ui.colored_label(colors::TEXT_SECONDARY, text);

                // SSE status
                if state.settings.health.sse_connected {
                    ui.add_space(12.0);
                    ui.colored_label(colors::SUCCESS, "SSE");
                }

                // Task status
                if let Some(task) = &state.task.active_task {
                    ui.add_space(12.0);
                    let task_text = format!("Task: {:?}", task.status);
                    ui.colored_label(colors::TEXT_MUTED, task_text);
                }

                // Total tokens / cost
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let total_tokens = state.total_token_usage();
                    let total_cost = state.total_estimated_cost();
                    if total_tokens > 0 {
                        ui.colored_label(
                            colors::TEXT_MUTED,
                            format!("~{} tokens | ${:.5}", total_tokens, total_cost),
                        );
                    }
                });
            });
        });
}