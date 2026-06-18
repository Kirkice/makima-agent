use eframe::egui::{self, Rounding};
use crate::state::chat_state::{ChatMessage, MessageType, SayKind, Session};
use crate::state::task_state::{TimelinePhase, TaskExecutionState};
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, session: &mut Session, task: &Option<TaskExecutionState>) {
    egui::Frame::none().fill(colors::GRAPHITE_BG).inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            // Task timeline
            if let Some(task_state) = task {
                if !task_state.timeline.is_empty() {
                    draw_timeline(ui, task_state);
                    ui.add_space(8.0);
                }
            }

            let messages = session.messages.clone();
            let total = messages.len();

            egui::ScrollArea::vertical().id_source("chat_transcript").auto_shrink([false, false]).stick_to_bottom(true)
                .show(ui, |ui| {
                    let mut copy_text: Option<String> = None;
                    let mut retry: bool = false;
                    for (idx, msg) in messages.iter().enumerate() {
                        draw_message_bubble(ui, msg, idx, &mut copy_text, &mut retry);
                        ui.add_space(4.0);
                    }

                    if total == 0 {
                        ui.add_space(32.0);
                        ui.vertical_centered(|ui| { ui.colored_label(colors::TEXT_MUTED, "Start a conversation"); });
                    }

                    // Handle copy / retry actions
                    if let Some(text) = copy_text {
                        ui.ctx().output_mut(|o| { o.copied_text = text; });
                    }
                    if retry {
                        // Retry: re-send last user message (handled via composer)
                    }
                });
        });
}

fn draw_message_bubble(ui: &mut egui::Ui, msg: &ChatMessage, _idx: usize, copy_text: &mut Option<String>, _retry: &mut bool) {
    let bg = match msg.msg_type {
        MessageType::Ask => colors::BUBBLE_USER_BG,
        MessageType::Say => match msg.say {
            Some(SayKind::Tool | SayKind::McpServerRequestStarted | SayKind::McpServerResponse) => colors::BUBBLE_TOOL_BG,
            _ => colors::BUBBLE_ASSISTANT_BG,
        },
    };
    let border = match msg.msg_type { MessageType::Ask => colors::RED_DIM, MessageType::Say => colors::GRAPHITE_BORDER };
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
    // Collapsed state stored in message - for now we use a local approach
    let collapse_id = ui.make_persistent_id(format!("collapsed_{:?}", msg.id));

    let mut collapsed = ui.memory_mut(|mem| *mem.data.get_temp_mut_or(collapse_id, false));
    let display_text = if is_tool && long_text && collapsed {
        format!("{}… ({} chars)", &text[..200], text.len())
    } else { text.clone() };

    let response = egui::Frame::none().fill(bg).stroke(egui::Stroke::new(1.0, border)).rounding(Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(colors::TEXT_ACCENT, role);
                if msg.partial { ui.colored_label(colors::TEXT_MUTED, " (streaming...)"); }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_tool && long_text {
                        let btn_label = if collapsed { "▸ Expand" } else { "▾ Collapse" };
                        if ui.small_button(btn_label).clicked() {
                            collapsed = !collapsed;
                            ui.memory_mut(|mem| mem.data.insert_temp(collapse_id, collapsed));
                        }
                    }
                });
            });
            ui.label(&display_text);
            if let Some(err) = &msg.error { ui.colored_label(colors::ERROR, err); }
            if let Some(tok) = msg.token_usage {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.colored_label(colors::TEXT_MUTED, format!("↑{} ↓{}", tok.total_tokens_in, tok.total_tokens_out));
                    if tok.total_cost > 0.0 { ui.colored_label(colors::TEXT_MUTED, format!("${:.5}", tok.total_cost)); }
                });
            }
        });

    // Right-click context menu
    response.response.context_menu(|ui| {
        if ui.button("📋 Copy").clicked() { *copy_text = Some(text.clone()); ui.close_menu(); }
        if ui.button("Retry").clicked() { ui.close_menu(); /* retry handled by caller */ }
    });
}

fn draw_timeline(ui: &mut egui::Ui, task: &TaskExecutionState) {
    egui::Frame::none().fill(colors::GRAPHITE_ELEVATED).rounding(Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(colors::TEXT_SECONDARY, "Task Timeline");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if task.elapsed_seconds > 0 {
                        ui.colored_label(colors::TEXT_MUTED, format!("{}s", task.elapsed_seconds));
                    }
                });
            });
            ui.separator();
            ui.add_space(4.0);

            for entry in &task.timeline {
                let color = match entry.phase {
                    TimelinePhase::Error => colors::ERROR, TimelinePhase::Completion => colors::SUCCESS,
                    TimelinePhase::ToolDispatch => colors::INFO, _ => colors::TEXT_SECONDARY,
                };
                let expand_id = ui.make_persistent_id(format!("tl_{:?}", entry.id));
                let mut expanded = ui.memory_mut(|mem| *mem.data.get_temp_mut_or(expand_id, false));

                ui.horizontal(|ui| {
                    ui.colored_label(color, entry.phase.icon());
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.colored_label(colors::TEXT_PRIMARY, &entry.label);
                        if expanded {
                            if let Some(detail) = &entry.detail { ui.colored_label(colors::TEXT_MUTED, detail); }
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if entry.detail.is_some() {
                            if ui.small_button(if expanded { "▾" } else { "▸" }).clicked() {
                                expanded = !expanded;
                                ui.memory_mut(|mem| mem.data.insert_temp(expand_id, expanded));
                            }
                        }
                    });
                });
                ui.add_space(2.0);
            }
        });
}