use eframe::egui::{self, Key};

use crate::app::UiAction;
use crate::state::app_state::AppState;
use crate::theme::colors;

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
            draw_top_row(ui, state);
            ui.add_space(6.0);

            let response = ui.add_sized(
                egui::vec2(ui.available_width(), 52.0),
                egui::TextEdit::multiline(&mut state.chat.composer.input)
                    .hint_text("Ask Makima anything...")
                    .desired_rows(3)
                    .frame(false),
            );

            ui.add_space(6.0);
            draw_bottom_row(ui, state, &mut should_send);

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

fn is_narrow(ui: &egui::Ui) -> bool {
    ui.available_width() < 260.0
}

fn draw_top_row(ui: &mut egui::Ui, state: &AppState) {
    if is_narrow(ui) {
        ui.vertical(|ui| {
            token_status(ui, state);
            stream_status(ui, state);
        });
    } else {
        ui.horizontal(|ui| {
            token_status(ui, state);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                stream_status(ui, state);
            });
        });
    }
}

fn token_status(ui: &mut egui::Ui, state: &AppState) {
    let tokens = estimate_tokens(&state.chat.composer.input);
    if tokens > 0 {
        let cost = (tokens as f64 / 1000.0) * state.settings.token_estimate_per_1k;
        let text = format!("~{tokens} tok | ${cost:.5}");
        ui.add(
            egui::Label::new(egui::RichText::new(text).size(11.0).color(colors::TEXT_MUTED)).wrap(),
        );
    } else {
        ui.colored_label(colors::TEXT_MUTED, egui::RichText::new("Ready").size(12.0));
    }
}

fn stream_status(ui: &mut egui::Ui, state: &AppState) {
    if state.chat.composer.is_streaming {
        ui.colored_label(
            colors::WARNING,
            egui::RichText::new("Streaming...").size(12.0),
        );
    } else {
        ui.colored_label(
            colors::TEXT_MUTED,
            egui::RichText::new("Ctrl+Enter to send").size(12.0),
        );
    }
}

fn draw_bottom_row(ui: &mut egui::Ui, state: &mut AppState, should_send: &mut bool) {
    if is_narrow(ui) {
        ui.vertical(|ui| {
            command_pills(ui);
            ui.add_space(4.0);
            action_button(ui, state, should_send);
        });
    } else {
        ui.horizontal(|ui| {
            command_pills(ui);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                action_button(ui, state, should_send);
            });
        });
    }
}

fn command_pills(ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        for cmd in ["/mode", "/clear", "/persona"] {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(cmd)
                        .size(11.0)
                        .color(colors::TEXT_MUTED)
                        .background_color(colors::SURFACE),
                )
                .sense(egui::Sense::hover()),
            );
            ui.add_space(4.0);
        }
    });
}

fn action_button(ui: &mut egui::Ui, state: &mut AppState, should_send: &mut bool) {
    if state.chat.composer.is_streaming {
        let stop_btn = egui::Button::new("Stop")
            .fill(colors::ERROR)
            .stroke(egui::Stroke::NONE)
            .min_size(egui::vec2(ui.available_width().min(96.0).max(72.0), 28.0));
        if ui.add_sized([ui.available_width(), 28.0], stop_btn).clicked() {
            state.chat.composer.is_streaming = false;
        }
    } else {
        let can_send = !state.chat.composer.input.trim().is_empty() && state.is_logged_in;
        let send_btn = egui::Button::new("Send")
            .fill(if can_send {
                colors::RED_ACCENT
            } else {
                colors::GRAPHITE_BORDER
            })
            .stroke(egui::Stroke::NONE)
            .min_size(egui::vec2(72.0, 28.0));
        if ui.add_enabled_ui(can_send, |ui| ui.add_sized([ui.available_width().min(96.0).max(72.0), 28.0], send_btn)).inner.clicked() {
            if handle_inline_command(state) {
                *should_send = true;
            }
        }
    }
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
