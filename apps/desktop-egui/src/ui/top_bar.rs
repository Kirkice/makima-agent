use eframe::egui::{self, CornerRadius};

use crate::state::app_state::{AppState, ViewMode};
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        egui::Frame::NONE
            .fill(colors::SELECTION_SOFT)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.colored_label(
                    colors::RED_ACCENT,
                    egui::RichText::new("M").size(11.0).strong(),
                );
            });

        ui.add_space(10.0);

        ui.vertical(|ui| {
            ui.colored_label(
                colors::TEXT_PRIMARY,
                egui::RichText::new("Makima").size(16.0).strong(),
            );
            ui.colored_label(
                colors::TEXT_MUTED,
                state
                    .chat
                    .active_session()
                    .map(|s| s.title.as_str())
                    .unwrap_or("New conversation"),
            );
        });

        ui.add_space(20.0);

        if state.is_logged_in {
            if let Some(mode) = state.settings.active_mode() {
                subtle_badge(ui, &mode.name);
            }
            if state.settings.model_config.configured {
                subtle_badge(ui, &state.settings.model_config.model);
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            connection_badge(ui, state);
            if state.is_logged_in {
                ui.add_space(12.0);
                draw_workspace_switch(ui, state);
            }
        });
    });
}

fn draw_workspace_switch(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(4, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                workspace_button(ui, state, ViewMode::Chat, "Chat");
                workspace_button(ui, state, ViewMode::Avatar, "Avatar");
            });
        });
}

fn workspace_button(ui: &mut egui::Ui, state: &mut AppState, mode: ViewMode, label: &str) {
    let active = state.view_mode == mode;
    let button = egui::Button::new(label)
        .fill(if active {
            colors::SELECTION_SOFT
        } else {
            colors::TRANSPARENT
        })
        .stroke(egui::Stroke::NONE);

    if ui.add(button).clicked() {
        state.view_mode = mode;
    }
}

fn connection_badge(ui: &mut egui::Ui, state: &AppState) {
    let (text, color) = if state.settings.health.backend && state.is_logged_in {
        ("Online", colors::SUCCESS)
    } else if state.settings.health.backend {
        ("Auth", colors::WARNING)
    } else {
        ("Offline", colors::TEXT_MUTED)
    };

    ui.horizontal(|ui| {
        ui.colored_label(color, "•");
        ui.colored_label(colors::TEXT_SECONDARY, text);
    });
}

fn subtle_badge(ui: &mut egui::Ui, text: &str) {
    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.colored_label(colors::TEXT_SECONDARY, text);
        });
}
