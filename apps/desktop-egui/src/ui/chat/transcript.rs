use eframe::egui::{self, CornerRadius};

use crate::state::chat_state::{ChatMessage, MessageType, SayKind, Session};
use crate::state::task_state::{TaskExecutionState, TimelinePhase};
use crate::theme::colors;

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

            egui::ScrollArea::vertical()
                .id_salt("chat_transcript")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    let mut copy_text = None;

                    for msg in &messages {
                        draw_message_bubble(ui, msg, &mut copy_text);
                        ui.add_space(8.0);
                    }

                    if total == 0 {
                        empty_stage(ui);
                    }

                    if let Some(text) = copy_text {
                        ui.ctx().copy_text(text);
                    }
                });
        });
}

fn empty_stage(ui: &mut egui::Ui) {
    ui.add_space(96.0);
    ui.vertical_centered(|ui| {
        ui.colored_label(colors::TEXT_MUTED, egui::RichText::new("◆").size(34.0));
        ui.add_space(12.0);
        ui.colored_label(
            colors::TEXT_PRIMARY,
            egui::RichText::new("Start a conversation").size(18.0).strong(),
        );
        ui.add_space(6.0);
        ui.colored_label(
            colors::TEXT_SECONDARY,
            "Use the composer below to begin a new thread.",
        );
    });
}

fn draw_message_bubble(ui: &mut egui::Ui, msg: &ChatMessage, copy_text: &mut Option<String>) {
    let (bg, border, role, accent) = match msg.msg_type {
        MessageType::Ask => (colors::BUBBLE_USER_BG, colors::RED_DIM, "You", colors::RED_ACCENT),
        MessageType::Say => match msg.say {
            Some(SayKind::Tool | SayKind::McpServerRequestStarted | SayKind::McpServerResponse) => {
                (colors::BUBBLE_TOOL_BG, colors::INFO, "Tool", colors::INFO)
            }
            Some(SayKind::Error) => (
                egui::Color32::from_rgb(57, 28, 32),
                colors::ERROR,
                "Error",
                colors::ERROR,
            ),
            Some(SayKind::Reasoning) => (
                colors::ELEVATED,
                colors::BORDER_WEAK,
                "Reasoning",
                colors::TEXT_SECONDARY,
            ),
            _ => (
                colors::BUBBLE_ASSISTANT_BG,
                colors::BORDER_WEAK,
                "Makima",
                colors::TEXT_PRIMARY,
            ),
        },
    };

    let text = msg.text.clone().unwrap_or_default();
    let is_tool = matches!(
        msg.say,
        Some(SayKind::Tool | SayKind::McpServerRequestStarted | SayKind::McpServerResponse)
    );

    let response = egui::Frame::NONE
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, border.linear_multiply(0.6)))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(
                    accent,
                    egui::RichText::new(role).size(12.0).strong(),
                );
                if msg.partial {
                    ui.colored_label(colors::TEXT_MUTED, "streaming");
                }
            });

            ui.add_space(6.0);
            ui.colored_label(colors::TEXT_PRIMARY, text.as_str());

            if let Some(err) = &msg.error {
                ui.add_space(8.0);
                ui.colored_label(colors::ERROR, err);
            }

            if let Some(tok) = msg.token_usage {
                ui.add_space(8.0);
                ui.colored_label(
                    colors::TEXT_MUTED,
                    format!(
                        "↑{}  ↓{}  ${:.5}",
                        tok.total_tokens_in, tok.total_tokens_out, tok.total_cost
                    ),
                );
            }

            if is_tool && text.len() > 320 {
                ui.add_space(6.0);
                ui.colored_label(colors::TEXT_MUTED, "Long tool output");
            }
        });

    response.response.context_menu(|ui| {
        if ui.button("Copy").clicked() {
            *copy_text = Some(text.clone());
            ui.close();
        }
    });
}

fn draw_timeline(ui: &mut egui::Ui, task: &TaskExecutionState) {
    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(colors::TEXT_PRIMARY, "Task Timeline");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if task.elapsed_seconds > 0 {
                        ui.colored_label(colors::TEXT_MUTED, format!("{}s", task.elapsed_seconds));
                    }
                });
            });

            ui.add_space(8.0);

            for entry in &task.timeline {
                let (tone, bg) = match entry.phase {
                    TimelinePhase::Error => (colors::ERROR, egui::Color32::from_rgb(57, 28, 32)),
                    TimelinePhase::Completion => {
                        (colors::SUCCESS, egui::Color32::from_rgb(24, 44, 34))
                    }
                    TimelinePhase::ToolDispatch => {
                        (colors::INFO, egui::Color32::from_rgb(22, 33, 49))
                    }
                    _ => (colors::TEXT_SECONDARY, colors::SURFACE),
                };

                egui::Frame::NONE
                    .fill(bg)
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(tone, entry.phase.icon());
                            ui.colored_label(colors::TEXT_PRIMARY, &entry.label);
                        });
                        if let Some(detail) = &entry.detail {
                            ui.colored_label(colors::TEXT_MUTED, detail);
                        }
                    });

                ui.add_space(4.0);
            }
        });
}
