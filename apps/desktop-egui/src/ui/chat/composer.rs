use eframe::egui::{self, Key};

use crate::state::app_state::AppState;
use crate::theme::colors;

/// Draw the chat input composer. Returns true on send.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) -> bool {
    let mut should_send = false;

    egui::Frame::none()
        .fill(colors::GRAPHITE_SURFACE)
        .rounding(egui::Rounding { nw: 8.0, ne: 8.0, sw: 0.0, se: 0.0 })
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            // Token estimate + Slash command hint
            ui.horizontal(|ui| {
                let tokens = estimate_tokens(&state.chat.composer.input);
                let cost = (tokens as f64 / 1000.0) * state.settings.token_estimate_per_1k;
                if tokens > 0 {
                    ui.colored_label(colors::TEXT_MUTED, format!("~{} tok | ${:.5}", tokens, cost));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if state.chat.composer.input.starts_with("/") {
                        let cmd = state.chat.composer.input.split_whitespace().next().unwrap_or("");
                        match cmd {
                            "/mode" => { ui.colored_label(colors::INFO, "→ Switch mode (usage: /mode <slug>)"); }
                            "/clear" => { ui.colored_label(colors::INFO, "→ Clear current conversation"); }
                            "/help" => { ui.colored_label(colors::INFO, "→ Available: /mode, /clear, /help, /persona"); }
                            "/persona" => { ui.colored_label(colors::INFO, "→ Reload persona"); }
                            _ => { ui.colored_label(colors::TEXT_MUTED, "Unknown command"); }
                        }
                    }
                });
            });

            let response = ui.add_sized(
                egui::vec2(ui.available_width(), 60.0),
                egui::TextEdit::multiline(&mut state.chat.composer.input)
                    .hint_text("Type a message... (Ctrl+Enter to send, drop files supported)")
                    .desired_rows(2)
                    .frame(true),
            );

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Stop button
                    if state.chat.composer.is_streaming {
                        if ui.button("■ Stop").clicked() { state.chat.composer.is_streaming = false; }
                    } else {
                        let can_send = !state.chat.composer.input.trim().is_empty() && state.is_logged_in;
                        if ui.add_enabled(can_send, egui::Button::new("▶ Send")).clicked() {
                            // Handle slash commands
                            if state.chat.composer.input.starts_with("/") {
                                let parts: Vec<&str> = state.chat.composer.input.split_whitespace().collect();
                                match parts[0] {
                                    "/clear" => {
                                        if let Some(s) = state.chat.active_session_mut() { s.messages.clear(); }
                                        state.chat.composer.input.clear();
                                        state.set_status("Conversation cleared".to_string());
                                        return;
                                    }
                                    "/help" => {
                                        state.set_status("Commands: /mode, /clear, /help, /persona".to_string());
                                        state.chat.composer.input.clear();
                                        return;
                                    }
                                    _ => { should_send = true; }
                                }
                            } else {
                                should_send = true;
                            }
                        }
                    }
                });
            });

            // Ctrl+Enter
            if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter) && i.modifiers.ctrl) {
                if !state.chat.composer.input.trim().is_empty() && !state.chat.composer.is_streaming { should_send = true; }
                response.request_focus();
            }

            // Drag-drop hint (egui 0.28 does not expose dropped_files in PlatformOutput)
            // Future: when egui 0.29+ is used, check ui.ctx().input(|i| i.raw.dropped_files)
        });

    should_send
}

fn estimate_tokens(text: &str) -> u64 {
    (text.len() as u64).div_ceil(4)
}