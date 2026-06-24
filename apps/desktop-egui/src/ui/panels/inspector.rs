use eframe::egui::{self, CornerRadius};

use crate::state::app_state::AppState;
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.vertical(|ui| {
        header(ui);
        ui.add_space(14.0);

        section_title(ui, "Agent");
        ui.add_space(6.0);

        kv_card(
            ui,
            "Mode",
            state
                .settings
                .active_mode()
                .map(|mode| mode.name.as_str())
                .unwrap_or("No mode selected"),
            colors::RED_ACCENT,
        );
        kv_card(
            ui,
            "Model",
            if state.settings.model_config.configured {
                &state.settings.model_config.model
            } else {
                "Not configured"
            },
            colors::INFO,
        );

        if let Some(session) = state.chat.active_session() {
            ui.add_space(12.0);
            section_title(ui, "Session");
            ui.add_space(6.0);

            kv_card(
                ui,
                "Messages",
                &format_count(session.messages.len(), "message", "messages"),
                colors::TEXT_MUTED,
            );
            let tokens = session.estimated_token_count();
            kv_card(ui, "Tokens", &format_tokens(tokens), colors::TEXT_MUTED);
            let cost = session.estimated_cost(state.settings.token_estimate_per_1k);
            kv_card(ui, "Est. Cost", &format!("${:.5}", cost), colors::TEXT_MUTED);
        }

        ui.add_space(12.0);
        section_title(ui, "Task");
        ui.add_space(6.0);

        if let Some(task) = &state.task.active_task {
            let (task_label, task_color) = match task.status {
                crate::state::task_state::TaskStatus::Running => ("Running", colors::SUCCESS),
                crate::state::task_state::TaskStatus::Idle => ("Idle", colors::TEXT_MUTED),
                _ => ("Completed", colors::INFO),
            };
            kv_card(ui, "Status", task_label, task_color);
            kv_card(
                ui,
                "Timeline",
                &format!(
                    "{} step{}",
                    task.timeline.len(),
                    if task.timeline.len() == 1 { "" } else { "s" }
                ),
                colors::TEXT_MUTED,
            );
        } else {
            kv_card(ui, "Status", "Idle", colors::TEXT_MUTED);
        }

        ui.add_space(12.0);
        section_title(ui, "Voice");
        ui.add_space(6.0);

        let (voice_label, voice_color) = if state.voice_call.is_connected {
            ("Connected", colors::SUCCESS)
        } else if state.voice_call.is_connecting {
            ("Connecting", colors::WARNING)
        } else {
            ("Idle", colors::TEXT_MUTED)
        };
        kv_card(ui, "Status", voice_label, voice_color);
    });
}

fn is_narrow(ui: &egui::Ui) -> bool {
    ui.available_width() < 210.0
}

fn header(ui: &mut egui::Ui) {
    ui.colored_label(
        colors::TEXT_PRIMARY,
        egui::RichText::new("Context").size(16.0).strong(),
    );
    ui.add(egui::Label::new(egui::RichText::new("Current session summary").color(colors::TEXT_MUTED)).wrap());

    let sep_rect = egui::Rect::from_min_size(
        ui.cursor().min + egui::vec2(0.0, 4.0),
        egui::vec2(ui.available_width(), 1.0),
    );
    ui.painter()
        .rect_filled(sep_rect, CornerRadius::ZERO, colors::BORDER_WEAK);
    ui.add_space(4.0);
}

fn section_title(ui: &mut egui::Ui, title: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(title)
                .size(if is_narrow(ui) { 11.0 } else { 12.0 })
                .strong()
                .color(colors::TEXT_SECONDARY),
        )
        .wrap(),
    );
}

fn kv_card(ui: &mut egui::Ui, label: &str, value: &str, accent: egui::Color32) {
    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin {
            left: 12,
            right: 12,
            top: 9,
            bottom: 9,
        })
        .show(ui, |ui| {
            let bar_rect = egui::Rect::from_min_size(
                ui.min_rect().min,
                egui::vec2(3.0, ui.min_rect().height()),
            );
            ui.painter()
                .rect_filled(bar_rect, CornerRadius::same(2), accent);

            if is_narrow(ui) {
                ui.vertical(|ui| {
                    ui.colored_label(colors::TEXT_MUTED, label);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(value)
                                .size(13.0)
                                .color(egui::Color32::WHITE),
                        )
                        .wrap(),
                    );
                });
            } else {
                ui.horizontal(|ui| {
                    ui.colored_label(colors::TEXT_MUTED, label);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(value)
                                    .size(13.0)
                                    .color(egui::Color32::WHITE),
                            )
                            .wrap(),
                        );
                    });
                });
            }
        });
    ui.add_space(6.0);
}

fn format_count(n: usize, singular: &str, plural: &str) -> String {
    if n >= 1000 {
        format!("{}K {}", n / 1000, plural)
    } else if n == 1 {
        format!("{} {}", n, singular)
    } else {
        format!("{} {}", n, plural)
    }
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
