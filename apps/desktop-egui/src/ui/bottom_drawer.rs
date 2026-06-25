//! Bottom Drawer - 按需浮现的复杂度容器
//!
//! 承载：TaskTimeline, VoiceCall, Audit, Diagnostics, McpActivity
//! 默认收起，仅在特定状态下自动展开。

use eframe::egui::{self, CornerRadius, Frame, Margin};
use crate::state::app_state::{AppState, DrawerTab};
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    let task_running = state.task.active_task.as_ref().map_or(false, |t| {
        t.status == crate::state::task_state::TaskStatus::Running
    });
    let voice_active = state.voice_call.is_connected || state.voice_call.is_connecting;

    // Reset dismissed flag when conditions change (so drawer can reopen for NEW events)
    if !task_running && !voice_active {
        state.drawer_user_dismissed = false;
    }

    // Auto-open rules (only when not user-dismissed)
    if !state.drawer_open && !state.drawer_user_dismissed {
        if task_running {
            state.drawer_open = true;
            state.drawer_tab = Some(DrawerTab::TaskTimeline);
        } else if voice_active {
            state.drawer_open = true;
            state.drawer_tab = Some(DrawerTab::VoiceCall);
        }
    }

    if !state.drawer_open {
        return;
    }

    Frame::NONE
        .fill(colors::SURFACE)
        .inner_margin(Margin::symmetric(12, 8))
        .corner_radius(CornerRadius::same(12))
        .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK))
        .show(ui, |ui| {
            ui.set_min_height(state.drawer_height);

            // Drawer header with tabs and close button
            ui.horizontal(|ui| {
                drawer_tab_button(ui, state, DrawerTab::TaskTimeline, "⏱ Timeline");
                drawer_tab_button(ui, state, DrawerTab::VoiceCall, "🎙 Voice");
                drawer_tab_button(ui, state, DrawerTab::Audit, "📊 Audit");
                drawer_tab_button(ui, state, DrawerTab::Diagnostics, "⚙️ Diagnostics");
                drawer_tab_button(ui, state, DrawerTab::McpActivity, "🔌 MCP");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕").clicked() {
                        state.drawer_open = false;
                        state.drawer_user_dismissed = true;
                    }
                });
            });

            ui.separator();
            ui.add_space(4.0);

            // Drawer content
            egui::ScrollArea::vertical()
                .max_height(state.drawer_height - 40.0)
                .show(ui, |ui| {
                    match state.drawer_tab {
                        Some(DrawerTab::TaskTimeline) => draw_task_timeline(ui, state),
                        Some(DrawerTab::VoiceCall) => draw_voice_call(ui, state),
                        Some(DrawerTab::Audit) => crate::ui::panels::audit::draw(ui, state),
                        Some(DrawerTab::Diagnostics) => crate::ui::panels::diagnostics::draw(ui, state),
                        Some(DrawerTab::McpActivity) => crate::ui::panels::mcp::draw(ui, state),
                        None => {
                            ui.colored_label(colors::TEXT_MUTED, "No drawer tab selected.");
                        }
                    }
                });
        });
}

fn drawer_tab_button(ui: &mut egui::Ui, state: &mut AppState, tab: DrawerTab, label: &str) {
    let active = state.drawer_tab == Some(tab);
    let style = if active {
        egui::Button::new(label).fill(colors::ELEVATED).small()
    } else {
        egui::Button::new(label).fill(colors::TRANSPARENT).small()
    };
    if ui.add(style).clicked() {
        state.drawer_tab = Some(tab);
    }
}

fn draw_task_timeline(ui: &mut egui::Ui, state: &AppState) {
    if let Some(task) = &state.task.active_task {
        ui.colored_label(colors::TEXT_PRIMARY, format!("Task: {:?}", task.status));
        ui.colored_label(colors::TEXT_MUTED, format!("Elapsed: {}s", task.elapsed_seconds));
        ui.add_space(4.0);

        for entry in &task.timeline {
            ui.horizontal(|ui| {
                ui.colored_label(colors::TEXT_SECONDARY, entry.phase.icon());
                ui.colored_label(colors::TEXT_PRIMARY, &entry.label);
                if let Some(detail) = &entry.detail {
                    ui.colored_label(colors::TEXT_MUTED, detail);
                }
            });
        }

        if task.timeline.is_empty() {
            ui.colored_label(colors::TEXT_MUTED, "No timeline entries yet.");
        }
    } else {
        ui.colored_label(colors::TEXT_MUTED, "No active task.");
    }
}

fn draw_voice_call(ui: &mut egui::Ui, state: &AppState) {
    let vc = &state.voice_call;
    ui.horizontal(|ui| {
        let (icon, color) = if vc.is_connected {
            ("●", colors::SUCCESS)
        } else if vc.is_connecting {
            ("◌", colors::WARNING)
        } else {
            ("○", colors::TEXT_MUTED)
        };
        ui.colored_label(color, icon);
        ui.colored_label(colors::TEXT_PRIMARY, &vc.status);
    });

    if vc.is_connected {
        let mins = vc.call_duration_secs / 60;
        let secs = vc.call_duration_secs % 60;
        ui.colored_label(colors::TEXT_SECONDARY, format!("Duration: {:02}:{:02}", mins, secs));
    }

    if let Some(err) = &vc.error {
        ui.colored_label(colors::ERROR, format!("⚠ {}", err));
    }
}

// Summary draw functions removed — bottom drawer now renders the full panels
// (audit::draw, diagnostics::draw, mcp::draw) directly for richer interaction.
