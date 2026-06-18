use crate::state::app_state::AppState;
use crate::theme::colors;
use eframe::egui;

/// Model configuration panel (Phase 3)
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.colored_label(colors::RED_ACCENT, "Model Configuration");
    ui.separator();
    ui.add_space(8.0);

    let model = &state.settings.model_config;

    metric(ui, "Provider", &model.provider);
    metric(ui, "Base URL", &model.base_url);
    metric(ui, "Model", &model.model);
    metric(ui, "Temperature", &format!("{:.2}", model.temperature));
    metric(ui, "Max Steps", &model.max_steps.to_string());
    metric(ui, "Timeout", &format!("{}s", model.timeout_seconds));

    let badge = if model.configured { ("Configured", colors::SUCCESS) } else { ("Not configured", colors::WARNING) };
    ui.colored_label(badge.1, badge.0);

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Test Connection").clicked() {
            state.set_status("Testing model connection...".to_string());
        }
        if ui.button("Edit").clicked() {
            state.show_modal_model_edit = true;
        }
    });

    // Edit modal (simplified)
    if state.show_modal_model_edit {
        egui::Window::new("Edit Model Config").resizable(false).collapsible(false)
            .show(ui.ctx(), |ui| {
                ui.label("Provider");
                ui.text_edit_singleline(&mut state.settings.model_config.provider);
                ui.label("Model");
                ui.text_edit_singleline(&mut state.settings.model_config.model);
                ui.label("Base URL");
                ui.text_edit_singleline(&mut state.settings.model_config.base_url);
                ui.add(egui::Slider::new(&mut state.settings.model_config.temperature, 0.0..=2.0).text("Temperature"));
                ui.add(egui::Slider::new(&mut state.settings.model_config.max_steps, 1..=500).text("Max Steps"));
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        state.settings.model_config.configured = true;
                        state.show_modal_model_edit = false;
                        state.set_status("Model config saved".to_string());
                    }
                    if ui.button("Cancel").clicked() { state.show_modal_model_edit = false; }
                });
            });
    }
}

fn metric(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(colors::TEXT_SECONDARY, label);
        ui.colored_label(colors::TEXT_PRIMARY, value);
    });
}