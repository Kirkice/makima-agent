use eframe::egui::{self, CornerRadius};

use crate::state::chat_state::Session;
use crate::state::task_state::{TaskExecutionState, TimelinePhase};
use crate::theme::colors;
use super::bubbles;

pub fn draw(ui: &mut egui::Ui, session: &mut Session, task: &Option<TaskExecutionState>) {
    egui::Frame::NONE
        .fill(colors::BG)
        .inner_margin(egui::Margin::symmetric(8, 8))
        .show(ui, |ui| {
            if let Some(task_state) = task {
                if !task_state.timeline.is_empty() {
                    draw_timeline(ui, task_state);
                    ui.add_space(12.0);
                }
            }

            let messages = session.messages.clone();
            let total = messages.len();
            // Capture viewport height before entering ScrollArea
            let viewport_h = ui.available_height();

            egui::ScrollArea::vertical()
                .id_salt("chat_transcript")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    let mut copy_text = None;

                    if total > 0 {
                        ui.add_space(4.0);
                    }
                    for msg in &messages {
                        // Route to Zoo-Code-inspired bubble components
                        bubbles::draw_message(ui, msg, &mut copy_text);
                        ui.add_space(6.0);
                    }

                    if total == 0 {
                        empty_stage(ui, viewport_h);
                    }

                    // Pad bottom for scroll comfort
                    ui.add_space(24.0);

                    if let Some(text) = copy_text {
                        ui.ctx().copy_text(text);
                    }
                });
        });
}

fn empty_stage(ui: &mut egui::Ui, viewport_h: f32) {
    let content_h = 24.0 + 8.0 + 18.0; // title + space + subtitle
    let top_pad = ((viewport_h - content_h) * 0.5).max(0.0);
    ui.add_space(top_pad);

    ui.vertical_centered(|ui| {
        ui.colored_label(
            colors::TEXT_PRIMARY,
            egui::RichText::new("Start a conversation").size(20.0).strong(),
        );
        ui.add_space(8.0);
        ui.colored_label(
            colors::TEXT_SECONDARY,
            egui::RichText::new("Use the composer below to begin a new thread.").size(14.0),
        );
    });
}

fn draw_timeline(ui: &mut egui::Ui, task: &TaskExecutionState) {
    let (status_color, status_label) = match task.status {
        crate::state::task_state::TaskStatus::Running => (colors::SUCCESS, "●  Running"),
        crate::state::task_state::TaskStatus::Idle => (colors::TEXT_MUTED, "○  Idle"),
        _ => (colors::INFO, "●  Complete"),
    };

    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(
                    colors::TEXT_SECONDARY,
                    egui::RichText::new("⚡  Task Timeline").size(13.0).strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(
                        status_color,
                        egui::RichText::new(status_label).size(12.0),
                    );
                    if task.elapsed_seconds > 0 {
                        ui.add_space(8.0);
                        ui.colored_label(
                            colors::TEXT_MUTED,
                            format!(
                                "{}s",
                                task.elapsed_seconds
                            ),
                        );
                    }
                });
            });

            if !task.timeline.is_empty() {
                ui.add_space(8.0);

                for entry in &task.timeline {
                    let (dot_color, bg) = match entry.phase {
                        TimelinePhase::Error => (
                            colors::ERROR,
                            egui::Color32::from_rgb(57, 28, 32),
                        ),
                        TimelinePhase::Completion => (
                            colors::SUCCESS,
                            egui::Color32::from_rgb(24, 44, 34),
                        ),
                        TimelinePhase::ToolDispatch => (
                            colors::INFO,
                            egui::Color32::from_rgb(22, 33, 49),
                        ),
                        _ => (colors::TEXT_SECONDARY, colors::SURFACE),
                    };

                    egui::Frame::NONE
                        .fill(bg)
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.colored_label(
                                    dot_color,
                                    egui::RichText::new(entry.phase.icon()).size(12.0),
                                );
                                ui.add_space(6.0);
                                ui.colored_label(
                                    colors::TEXT_PRIMARY,
                                    egui::RichText::new(&entry.label).size(12.0),
                                );
                            });
                            if let Some(detail) = &entry.detail {
                                if !detail.is_empty() {
                                    ui.colored_label(
                                        colors::TEXT_MUTED,
                                        egui::RichText::new(detail).size(11.0),
                                    );
                                }
                            }
                        });

                    ui.add_space(3.0);
                }
            }
        });
}