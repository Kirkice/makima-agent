use crate::state::app_state::AppState;
use crate::state::settings_state::ModeConfig;
use crate::theme::colors;

use eframe::egui;

/// Full mode management panel (Phase 2)
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.colored_label(colors::RED_ACCENT, "Mode Management");
    ui.separator();
    ui.add_space(8.0);

    // Active mode display
    if let Some(mode) = state.settings.active_mode() {
        section(ui, "Active Mode", colors::SUCCESS, &mode.name);
        ui.label(&mode.role_definition);
        if let Some(wtu) = &mode.when_to_use {
            ui.colored_label(colors::TEXT_MUTED, format!("When to use: {}", wtu));
        }
    } else {
        ui.colored_label(colors::TEXT_MUTED, "No active mode");
    }

    ui.separator();

    // Mode list with details
    ui.horizontal(|ui| {
        ui.colored_label(colors::TEXT_SECONDARY, "Available Modes");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Reload").clicked() {
                state.show_modal_mode_reload = true;
            }
            if ui.button("New").clicked() {
                state.show_modal_mode_create = true;
            }
        });
    });

    let modes = state.settings.modes.clone();
    for mode in &modes {
        let is_active = Some(&mode.slug) == state.settings.active_mode_slug.as_ref();
        let bg = if is_active { colors::RED_DIM } else { colors::GRAPHITE_ELEVATED };

        egui::Frame::NONE.fill(bg).corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if is_active { ui.colored_label(colors::RED_ACCENT, "●"); }
                    ui.vertical(|ui| {
                        ui.colored_label(colors::TEXT_PRIMARY, &mode.name);
                        ui.colored_label(colors::TEXT_MUTED, &mode.slug);
                        if let Some(d) = &mode.description {
                            ui.colored_label(colors::TEXT_MUTED, d);
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !is_active && ui.button("Select").clicked() {
                            state.settings.active_mode_slug = Some(mode.slug.clone());
                        }
                        if ui.button("×").clicked() {
                            state.settings.modes.retain(|m| m.slug != mode.slug);
                        }
                    });
                });
            });
        ui.add_space(4.0);
    }

    // Reload modal
    if state.show_modal_mode_reload {
        egui::Window::new("Reload Modes").collapsible(false).resizable(false)
            .show(ui.ctx(), |ui| {
                ui.label("Reload modes from configuration?");
                ui.horizontal(|ui| {
                    if ui.button("Yes").clicked() { state.show_modal_mode_reload = false; state.set_status("Reloading modes...".to_string()); }
                    if ui.button("Cancel").clicked() { state.show_modal_mode_reload = false; }
                });
            });
    }

    // Create modal (simplified)
    if state.show_modal_mode_create {
        egui::Window::new("Create Mode").collapsible(false).resizable(false)
            .show(ui.ctx(), |ui| {
                ui.label("Create: (dialog placeholder, extend later)");
                if ui.button("Close").clicked() { state.show_modal_mode_create = false; }
            });
    }
}

fn section(ui: &mut egui::Ui, title: &str, color: egui::Color32, value: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(color, title);
        ui.colored_label(colors::TEXT_PRIMARY, value);
    });
}