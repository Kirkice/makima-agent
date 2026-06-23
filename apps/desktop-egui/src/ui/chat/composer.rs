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
        .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK.linear_multiply(0.5)))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            // ── Top bar: token estimate + shortcuts ──
            ui.horizontal(|ui| {
                let tokens = estimate_tokens(&state.chat.composer.input);
                if tokens > 0 {
                    let cost = (tokens as f64 / 1000.0) * state.settings.token_estimate_per_1k;
                    // Token pill
                    let pill_text = format!("~{tokens} tok  ·  ${cost:.5}");
                    let pill_galley = ui.painter().layout_no_wrap(
                        pill_text.clone(),
                        egui::FontId::proportional(11.0),
                        colors::TEXT_MUTED,
                    );
                    let pill_size = pill_galley.size() + egui::vec2(10.0, 4.0);
                    let (pill_rect, _) =
                        ui.allocate_exact_size(pill_size, egui::Sense::hover());
                    ui.painter().rect_filled(
                        pill_rect,
                        egui::CornerRadius::same(4),
                        colors::SURFACE,
                    );
                    ui.painter().galley(
                        pill_rect.center() - pill_galley.size() * 0.5,
                        pill_galley,
                        colors::TEXT_SECONDARY,
                    );
                } else {
                    ui.colored_label(colors::TEXT_MUTED, egui::RichText::new("Ready").size(12.0));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if state.chat.composer.is_streaming {
                        ui.colored_label(
                            colors::WARNING,
                            egui::RichText::new("◌  Streaming...").size(12.0),
                        );
                    } else {
                        ui.colored_label(
                            colors::TEXT_MUTED,
                            egui::RichText::new("Ctrl+Enter to send").size(12.0),
                        );
                    }
                });
            });

            ui.add_space(6.0);

            // ── Text input area ──
            let response = ui.add_sized(
                egui::vec2(ui.available_width(), 52.0),
                egui::TextEdit::multiline(&mut state.chat.composer.input)
                    .hint_text("Ask Makima anything…")
                    .desired_rows(3)
                    .frame(false),
            );

            ui.add_space(4.0);

            // ── Bottom bar: slash hints + send ──
            ui.horizontal(|ui| {
                // Slash command hints as small pills
                let commands = ["/mode", "/clear", "/persona"];
                for cmd in &commands {
                    let galley = ui.painter().layout_no_wrap(
                        cmd.to_string(),
                        egui::FontId::proportional(11.0),
                        colors::TEXT_MUTED,
                    );
                    let pill = galley.size() + egui::vec2(8.0, 3.0);
                    let (r, _) = ui.allocate_exact_size(pill, egui::Sense::hover());
                    ui.painter().rect_filled(
                        r,
                        egui::CornerRadius::same(3),
                        colors::SURFACE,
                    );
                    ui.painter().galley(
                        r.center() - galley.size() * 0.5,
                        galley,
                        colors::TEXT_MUTED,
                    );
                    ui.add_space(4.0);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if state.chat.composer.is_streaming {
                        let stop_btn = egui::Button::new("⏹  Stop")
                            .fill(colors::ERROR)
                            .stroke(egui::Stroke::NONE)
                            .min_size(egui::vec2(72.0, 28.0));
                        if ui.add(stop_btn).clicked() {
                            state.chat.composer.is_streaming = false;
                        }
                    } else {
                        let can_send =
                            !state.chat.composer.input.trim().is_empty() && state.is_logged_in;
                        let send_btn = egui::Button::new("↑  Send")
                            .fill(if can_send {
                                colors::RED_ACCENT
                            } else {
                                colors::GRAPHITE_BORDER
                            })
                            .stroke(egui::Stroke::NONE)
                            .min_size(egui::vec2(72.0, 28.0));
                        if ui.add_enabled(can_send, send_btn).clicked() {
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