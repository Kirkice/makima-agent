use eframe::egui::{self, CornerRadius};
use crate::state::chat_state::{ChatMessage, MessageType, SayKind, Session};
use crate::state::task_state::{TimelinePhase, TaskExecutionState};
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, session: &mut Session, task: &Option<TaskExecutionState>) {
    egui::Frame::NONE
        .fill(colors::GRAPHITE_BG)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            // Task timeline
            if let Some(task_state) = task {
                if !task_state.timeline.is_empty() {
                    draw_timeline(ui, task_state);
                    ui.add_space(12.0);
                }
            }

            let messages = session.messages.clone();
            let total = messages.len();

            egui::ScrollArea::vertical()
                .id_source("chat_transcript")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    let mut copy_text: Option<String> = None;
                    let mut retry: bool = false;
                    
                    for (idx, msg) in messages.iter().enumerate() {
                        draw_message_bubble(ui, msg, idx, &mut copy_text, &mut retry);
                        ui.add_space(6.0);
                    }

                    if total == 0 {
                        ui.add_space(64.0);
                        ui.vertical_centered(|ui| {
                            ui.colored_label(
                                colors::TEXT_MUTED,
                                egui::RichText::new("💬").size(48.0)
                            );
                            ui.add_space(12.0);
                            ui.colored_label(
                                colors::TEXT_PRIMARY,
                                egui::RichText::new("Start a conversation").size(16.0).strong()
                            );
                            ui.add_space(4.0);
                            ui.colored_label(
                                colors::TEXT_SECONDARY,
                                "Type a message below to begin"
                            );
                        });
                    }

                    // Handle copy / retry actions
                    if let Some(text) = copy_text {
                        ui.ctx().copy_text(text);
                    }
                    if retry {
                        // Retry: re-send last user message (handled via composer)
                    }
                });
        });
}

fn draw_message_bubble(ui: &mut egui::Ui, msg: &ChatMessage, _idx: usize, copy_text: &mut Option<String>, _retry: &mut bool) {
    let (bg, border, role_icon, role_color) = match msg.msg_type {
        MessageType::Ask => (
            colors::BUBBLE_USER_BG,
            colors::RED_DIM,
            "👤",
            colors::RED_ACCENT
        ),
        MessageType::Say => match msg.say {
            Some(SayKind::Tool | SayKind::McpServerRequestStarted | SayKind::McpServerResponse) => (
                colors::BUBBLE_TOOL_BG,
                egui::Color32::from_rgb(60, 80, 100),
                "🔧",
                colors::INFO
            ),
            Some(SayKind::Error) => (
                egui::Color32::from_rgb(40, 20, 20),
                colors::ERROR,
                "❌",
                colors::ERROR
            ),
            Some(SayKind::Reasoning) => (
                egui::Color32::from_rgb(30, 30, 40),
                egui::Color32::from_rgb(120, 120, 140),
                "💭",
                colors::TEXT_SECONDARY
            ),
            _ => (
                colors::BUBBLE_ASSISTANT_BG,
                egui::Color32::from_rgb(50, 50, 60),
                "🤖",
                colors::SUCCESS
            ),
        },
    };
    
    let role = match msg.msg_type {
        MessageType::Ask => "You",
        MessageType::Say => match msg.say {
            Some(SayKind::Tool | SayKind::McpServerRequestStarted) => "Tool",
            Some(SayKind::Error) => "Error",
            Some(SayKind::Reasoning) => "Reasoning",
            _ => "Makima",
        },
    };

    let text = msg.text.clone().unwrap_or_default();
    let is_tool = matches!(msg.say, Some(SayKind::Tool | SayKind::McpServerRequestStarted | SayKind::McpServerResponse));
    let long_text = text.len() > 300;
    let collapse_id = ui.make_persistent_id(format!("collapsed_{:?}", msg.id));

    let mut collapsed = ui.memory_mut(|mem| *mem.data.get_temp_mut_or(collapse_id, false));
    let display_text = if is_tool && long_text && collapsed {
        format!("{}… ({} chars)", &text[..200], text.len())
    } else { text.clone() };

    let response = egui::Frame::NONE
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .shadow(egui::epaint::Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: egui::Color32::BLACK.linear_multiply(0.15),
        })
        .show(ui, |ui| {
            // Header with role icon and name
            ui.horizontal(|ui| {
                ui.colored_label(role_color, egui::RichText::new(role_icon).size(16.0));
                ui.add_space(6.0);
                ui.colored_label(
                    role_color,
                    egui::RichText::new(role).strong().size(13.0)
                );
                if msg.partial {
                    ui.add_space(4.0);
                    ui.colored_label(
                        colors::TEXT_MUTED,
                        egui::RichText::new("● streaming...").size(11.0)
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_tool && long_text {
                        let btn_label = if collapsed { "▸ Expand" } else { "▾ Collapse" };
                        let btn = egui::Button::new(btn_label)
                            .fill(colors::GRAPHITE_ELEVATED)
                            .stroke(egui::Stroke::new(1.0, colors::GRAPHITE_BORDER));
                        if ui.add(btn).clicked() {
                            collapsed = !collapsed;
                            ui.memory_mut(|mem| mem.data.insert_temp(collapse_id, collapsed));
                        }
                    }
                });
            });
            
            ui.add_space(8.0);
            
            // Message content
            ui.colored_label(
                colors::TEXT_PRIMARY,
                egui::RichText::new(&display_text).size(13.0)
            );
            
            // Error message
            if let Some(err) = &msg.error {
                ui.add_space(6.0);
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgb(60, 20, 20))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(colors::ERROR, "⚠️");
                            ui.colored_label(colors::ERROR, err);
                        });
                    });
            }
            
            // Token usage
            if let Some(tok) = msg.token_usage {
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.colored_label(colors::TEXT_MUTED, egui::RichText::new("📊").size(11.0));
                    ui.add_space(4.0);
                    ui.colored_label(
                        colors::TEXT_SECONDARY,
                        egui::RichText::new(format!("Tokens: ↑{} ↓{}", tok.total_tokens_in, tok.total_tokens_out)).size(11.0)
                    );
                    if tok.total_cost > 0.0 {
                        ui.add_space(8.0);
                        ui.colored_label(
                            colors::WARNING,
                            egui::RichText::new(format!("${:.5}", tok.total_cost)).size(11.0).strong()
                        );
                    }
                });
            }
        });

    // Right-click context menu
    response.response.context_menu(|ui| {
        if ui.button("📋 Copy").clicked() { *copy_text = Some(text.clone()); ui.close_menu(); }
        if ui.button("🔄 Retry").clicked() { ui.close_menu(); /* retry handled by caller */ }
    });
}

fn draw_timeline(ui: &mut egui::Ui, task: &TaskExecutionState) {
    egui::Frame::NONE
        .fill(colors::GRAPHITE_ELEVATED)
        .stroke(egui::Stroke::new(1.0, colors::GRAPHITE_BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            // Header
            ui.horizontal(|ui| {
                ui.colored_label(colors::RED_ACCENT, "⏱️");
                ui.add_space(6.0);
                ui.colored_label(
                    colors::TEXT_PRIMARY,
                    egui::RichText::new("Task Timeline").strong().size(13.0)
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if task.elapsed_seconds > 0 {
                        egui::Frame::NONE
                            .fill(colors::GRAPHITE_SURFACE)
                            .corner_radius(CornerRadius::same(4))
                            .inner_margin(egui::Margin::symmetric(8, 2))
                            .show(ui, |ui| {
                                ui.colored_label(
                                    colors::TEXT_SECONDARY,
                                    egui::RichText::new(format!("{}s", task.elapsed_seconds)).size(11.0)
                                );
                            });
                    }
                });
            });
            
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            for entry in &task.timeline {
                let (color, bg) = match entry.phase {
                    TimelinePhase::Error => (colors::ERROR, egui::Color32::from_rgb(60, 20, 20)),
                    TimelinePhase::Completion => (colors::SUCCESS, egui::Color32::from_rgb(20, 60, 30)),
                    TimelinePhase::ToolDispatch => (colors::INFO, egui::Color32::from_rgb(20, 40, 60)),
                    _ => (colors::TEXT_SECONDARY, colors::GRAPHITE_SURFACE),
                };
                
                let expand_id = ui.make_persistent_id(format!("tl_{:?}", entry.id));
                let mut expanded = ui.memory_mut(|mem| *mem.data.get_temp_mut_or(expand_id, false));

                egui::Frame::NONE
                    .fill(bg)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(color, egui::RichText::new(entry.phase.icon()).size(14.0));
                            ui.add_space(6.0);
                            ui.vertical(|ui| {
                                ui.colored_label(
                                    colors::TEXT_PRIMARY,
                                    egui::RichText::new(&entry.label).size(12.0)
                                );
                                if expanded {
                                    if let Some(detail) = &entry.detail {
                                        ui.add_space(2.0);
                                        ui.colored_label(
                                            colors::TEXT_SECONDARY,
                                            egui::RichText::new(detail).size(11.0)
                                        );
                                    }
                                }
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if entry.detail.is_some() {
                                    let btn = egui::Button::new(if expanded { "▾" } else { "▸" })
                                        .fill(colors::GRAPHITE_ELEVATED);
                                    if ui.add(btn).clicked() {
                                        expanded = !expanded;
                                        ui.memory_mut(|mem| mem.data.insert_temp(expand_id, expanded));
                                    }
                                }
                            });
                        });
                    });
                ui.add_space(3.0);
            }
        });
}
