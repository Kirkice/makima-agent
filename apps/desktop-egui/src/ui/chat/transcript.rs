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
                        draw_message_bubble(ui, msg, &mut copy_text);
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

fn draw_message_bubble(ui: &mut egui::Ui, msg: &ChatMessage, copy_text: &mut Option<String>) {
    let (bg, accent, role, role_color) = match msg.msg_type {
        MessageType::Ask => (
            colors::BUBBLE_USER_BG,
            colors::RED_ACCENT,
            "You",
            colors::RED_ACCENT,
        ),
        MessageType::Say => match msg.say {
            Some(SayKind::Tool | SayKind::McpServerRequestStarted | SayKind::McpServerResponse) => {
                (colors::BUBBLE_TOOL_BG, colors::INFO, "⚙  Tool", colors::INFO)
            }
            Some(SayKind::Error) => (
                egui::Color32::from_rgb(57, 28, 32),
                colors::ERROR,
                "✖  Error",
                colors::ERROR,
            ),
            Some(SayKind::Reasoning) => (
                colors::ELEVATED,
                colors::TEXT_SECONDARY,
                "💭  Reasoning",
                colors::TEXT_SECONDARY,
            ),
            _ => (
                colors::BUBBLE_ASSISTANT_BG,
                colors::TEXT_PRIMARY,
                "Makima",
                colors::TEXT_PRIMARY,
            ),
        },
    };

    let text = msg.text.clone().unwrap_or_default();

    let response = egui::Frame::NONE
        .fill(bg)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin {
            left: 14,
            right: 14,
            top: 10,
            bottom: 10,
        })
        .show(ui, |ui| {
            // Left accent bar
            let bar_rect = egui::Rect::from_min_size(
                ui.min_rect().min,
                egui::vec2(3.0, ui.min_rect().height()),
            );
            ui.painter()
                .rect_filled(bar_rect, CornerRadius::same(2), accent);

            // Role header
            ui.horizontal(|ui| {
                // Colored dot for role indicator
                let (dot_rect, _) =
                    ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter()
                    .circle_filled(dot_rect.center(), 4.0, accent);
                ui.add_space(4.0);
                ui.colored_label(
                    role_color,
                    egui::RichText::new(role).size(12.0).strong(),
                );
                if msg.partial {
                    ui.add_space(6.0);
                    ui.colored_label(
                        colors::WARNING,
                        egui::RichText::new("◌ streaming").size(11.0),
                    );
                }
            });

            ui.add_space(6.0);

            // Message body
            if text.len() > 600 {
                // Long messages: show with max height + scroll
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        ui.colored_label(
                            colors::TEXT_PRIMARY,
                            egui::RichText::new(&text).size(13.5),
                        );
                    });
            } else {
                ui.colored_label(
                    colors::TEXT_PRIMARY,
                    egui::RichText::new(&text).size(13.5),
                );
            }

            if let Some(err) = &msg.error {
                ui.add_space(8.0);
                ui.colored_label(colors::ERROR, egui::RichText::new(err).size(12.0));
            }

            if let Some(tok) = msg.token_usage {
                ui.add_space(8.0);
                let usage_text = format!(
                    "↑ {}  ↓ {}  ·  ${:.5}",
                    format_tokens(tok.total_tokens_in),
                    format_tokens(tok.total_tokens_out),
                    tok.total_cost
                );
                ui.colored_label(
                    colors::TEXT_MUTED,
                    egui::RichText::new(usage_text).size(11.0),
                );
            }
        });

    response.response.context_menu(|ui| {
        if ui.button("📋  Copy message").clicked() {
            *copy_text = Some(text.clone());
            ui.close();
        }
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

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1000 {
        format!("{:.1}K", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}