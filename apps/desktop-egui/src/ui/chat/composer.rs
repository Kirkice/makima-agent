use eframe::egui::{self, Key};

use crate::app::UiAction;
use crate::state::app_state::AppState;
use crate::theme::colors;

/// Draw the chat input composer. Returns true on send.
pub fn draw(
    ui: &mut egui::Ui,
    state: &mut AppState,
    _pending_action: &mut Option<UiAction>,
) -> bool {
    let mut should_send = false;

    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let tokens = estimate_tokens(&state.chat.composer.input);
                if tokens > 0 {
                    let cost = (tokens as f64 / 1000.0) * state.settings.token_estimate_per_1k;
                    ui.colored_label(colors::TEXT_MUTED, format!("~{tokens} tok · ${cost:.5}"));
                } else {
                    ui.colored_label(colors::TEXT_MUTED, "Ready");
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(colors::TEXT_MUTED, "Ctrl+Enter to send");
                });
            });

            let response = ui.add_sized(
                egui::vec2(ui.available_width(), 56.0),
                egui::TextEdit::multiline(&mut state.chat.composer.input)
                    .hint_text("Ask Makima anything…")
                    .desired_rows(3)
                    .frame(false),
            );

            ui.horizontal(|ui| {
                ui.colored_label(colors::TEXT_MUTED, "/mode  /clear  /persona");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if state.chat.composer.is_streaming {
                        if ui.button("Stop").clicked() {
                            state.chat.composer.is_streaming = false;
                        }
                    } else {
                        let send = egui::Button::new("Send")
                            .fill(colors::SELECTION_SOFT)
                            .stroke(egui::Stroke::NONE);
                        let can_send =
                            !state.chat.composer.input.trim().is_empty() && state.is_logged_in;
                        if ui.add_enabled(can_send, send).clicked() {
                            if handle_inline_command(state) {
                                should_send = true;
                            }
                        }
                    }
                });
            });

            if response.lost_focus()
                && ui.input(|i| i.key_pressed(Key::Enter) && i.modifiers.ctrl)
                && !state.chat.composer.input.trim().is_empty()
                && !state.chat.composer.is_streaming
            {
                if handle_inline_command(state) {
                    should_send = true;
                }
                response.request_focus();
            }
        });

    should_send
}

fn handle_inline_command(state: &mut AppState) -> bool {
    if !state.chat.composer.input.starts_with('/') {
        return true;
    }

    let parts: Vec<&str> = state.chat.composer.input.split_whitespace().collect();
    if parts.is_empty() {
        return false;
    }

    match parts[0] {
        "/clear" => {
            if let Some(session) = state.chat.active_session_mut() {
                session.messages.clear();
            }
            state.chat.composer.input.clear();
            state.set_status("Conversation cleared".to_string());
            false
        }
        "/help" => {
            state.set_status("Commands: /mode, /clear, /help, /persona".to_string());
            state.chat.composer.input.clear();
            false
        }
        _ => true,
    }
}

fn estimate_tokens(text: &str) -> u64 {
    (text.len() as u64).div_ceil(4)
}
