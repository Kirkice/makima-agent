use eframe::egui::{self, CornerRadius};

use crate::state::app_state::{ApiCommand, AppState};
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.vertical(|ui| {
        section_header(ui, "Conversations", "Recent chats and active threads");
        ui.add_space(12.0);

        draw_sessions(ui, state);
    });
}

fn is_narrow(ui: &egui::Ui) -> bool {
    ui.available_width() < 220.0
}

fn draw_sessions(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            let new_button = egui::Button::new("+ New")
                .fill(colors::RED_ACCENT)
                .stroke(egui::Stroke::NONE)
                .min_size(egui::vec2(64.0, 26.0));

            if is_narrow(ui) {
                ui.vertical(|ui| {
                    ui.colored_label(
                        colors::TEXT_PRIMARY,
                        egui::RichText::new("Recent").size(13.0).strong(),
                    );
                    if ui.add_sized([ui.available_width(), 26.0], new_button).clicked() {
                        state
                            .chat
                            .create_session(format!("Chat {}", state.chat.sessions.len() + 1));
                        state.set_status("New session created".to_string());
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        colors::TEXT_PRIMARY,
                        egui::RichText::new("Recent").size(13.0).strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(new_button).clicked() {
                            state
                                .chat
                                .create_session(format!("Chat {}", state.chat.sessions.len() + 1));
                            state.set_status("New session created".to_string());
                        }
                    });
                });
            }
        });
    ui.add_space(8.0);

    let active_id = state.chat.active_session_id;
    let is_empty = state.chat.sessions.is_empty();
    let mut to_select = None;

    if !is_empty {
        draw_search(ui, &mut state.chat.search_query, "Search conversations");
        ui.add_space(10.0);
    }

    let available = ui.available_size_before_wrap();
    let scroll_h = (available.y - 4.0).max(0.0);
    egui::ScrollArea::vertical()
        .id_salt("conversation_sidebar_list")
        .auto_shrink([false, false])
        .max_height(scroll_h)
        .show(ui, |ui| {
            let mut to_delete = None;

            for session in &state.chat.sessions {
                let matches = state.chat.search_query.is_empty()
                    || session
                        .title
                        .to_lowercase()
                        .contains(&state.chat.search_query.to_lowercase());
                if !matches {
                    continue;
                }

                let selected = Some(session.id) == active_id;
                let response = egui::Frame::NONE
                    .fill(if selected {
                        colors::SELECTION_SOFT
                    } else {
                        colors::TRANSPARENT
                    })
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        if ui.available_width() < 170.0 {
                            ui.vertical(|ui| {
                                let label_w = ui.available_width().max(60.0);
                                ui.colored_label(
                                    colors::TEXT_PRIMARY,
                                    egui::RichText::new(truncate_to_width(&session.title, label_w)).size(13.0),
                                );
                                ui.colored_label(
                                    colors::TEXT_MUTED,
                                    egui::RichText::new(format!("{} msg", session.messages.len())).size(11.0),
                                );
                                let del_btn = egui::Button::new("🗑")
                                    .fill(colors::TRANSPARENT)
                                    .stroke(egui::Stroke::NONE)
                                    .min_size(egui::vec2(22.0, 22.0));
                                if ui.add(del_btn).on_hover_text("Delete conversation").clicked() {
                                    to_delete = Some(session.id);
                                }
                            });
                        } else {
                            ui.horizontal(|ui| {
                                if session.unread {
                                    let (dot_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(6.0, 6.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter()
                                        .circle_filled(dot_rect.center(), 3.0, colors::RED_ACCENT);
                                    ui.add_space(6.0);
                                } else {
                                    ui.add_sized(egui::vec2(6.0, 6.0), egui::Label::new(""));
                                    ui.add_space(6.0);
                                }

                                let label_w = (ui.available_width() - 30.0).max(40.0);
                                ui.vertical(|ui| {
                                    ui.set_width(label_w);
                                    ui.colored_label(
                                        colors::TEXT_PRIMARY,
                                        egui::RichText::new(truncate_to_width(&session.title, label_w)).size(13.0),
                                    );
                                    ui.colored_label(
                                        colors::TEXT_MUTED,
                                        egui::RichText::new(format!("{} msg", session.messages.len())).size(11.0),
                                    );
                                });

                                let del_btn = egui::Button::new("🗑")
                                    .fill(colors::TRANSPARENT)
                                    .stroke(egui::Stroke::NONE)
                                    .min_size(egui::vec2(22.0, 22.0));
                                if ui.add(del_btn).on_hover_text("Delete conversation").clicked() {
                                    to_delete = Some(session.id);
                                }
                            });
                        }
                    });

                if response.response.clicked() {
                    to_select = Some(session.id);
                }
                ui.add_space(4.0);
            }

            if let Some(delete_id) = to_delete {
                let id_str = delete_id.to_string();
                state.api_commands.push(ApiCommand::DeleteSession(id_str));
                state.chat.delete_session(delete_id);
                state.set_status("Conversation deleted".to_string());
            }
        });

    if let Some(id) = to_select {
        state.chat.active_session_id = Some(id);
        if let Some(session) = state.chat.active_session_mut() {
            session.unread = false;
        }
    }
}

fn section_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.vertical(|ui| {
        ui.colored_label(
            colors::TEXT_PRIMARY,
            egui::RichText::new(title).size(16.0).strong(),
        );
        ui.add(egui::Label::new(egui::RichText::new(subtitle).color(colors::TEXT_MUTED)).wrap());

        let sep_rect = egui::Rect::from_min_size(
            ui.cursor().min + egui::vec2(0.0, 4.0),
            egui::vec2(ui.available_width(), 1.0),
        );
        ui.painter()
            .rect_filled(sep_rect, CornerRadius::ZERO, colors::BORDER_WEAK);
    });
}

fn draw_search(ui: &mut egui::Ui, value: &mut String, hint: &str) {
    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            if is_narrow(ui) {
                ui.vertical(|ui| {
                    ui.colored_label(colors::TEXT_MUTED, "Search");
                    ui.add(
                        egui::TextEdit::singleline(value)
                            .hint_text(hint)
                            .frame(false)
                            .desired_width(f32::INFINITY),
                    );
                });
            } else {
                ui.horizontal(|ui| {
                    ui.colored_label(colors::TEXT_MUTED, "Search");
                    ui.add_space(8.0);
                    ui.add(
                        egui::TextEdit::singleline(value)
                            .hint_text(hint)
                            .frame(false)
                            .desired_width(f32::INFINITY),
                    );
                });
            }
        });
}

fn truncate_to_width(s: &str, max_width: f32) -> String {
    let avg_char_w = 8.0;
    let max_chars = (max_width / avg_char_w).max(1.0) as usize;
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    }
}