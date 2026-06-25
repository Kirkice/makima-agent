use crate::state::app_state::{ApiCommand, AppState};
use crate::theme::colors;
use eframe::egui;

/// Persona management panel — wired to backend via ApiCommand channel.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.colored_label(colors::RED_ACCENT, "Persona");
    ui.separator();
    ui.add_space(8.0);

    section(ui, "Current Persona");
    ui.colored_label(colors::TEXT_PRIMARY, state.settings.persona_name.clone());
    if state.settings.persona_is_default {
        ui.colored_label(colors::SUCCESS, "✓ Default persona");
    }
    if state.settings.persona_modified {
        ui.colored_label(colors::WARNING, "⚠ In-memory only (needs .makima/persona.yaml update)");
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Reload Persona").clicked() {
            state.api_commands.push(ApiCommand::ReloadPersona);
            state.set_status("Reloading persona...".to_string());
        }
        if ui.button("View Default").clicked() {
            state.show_persona_default = !state.show_persona_default;
        }
        if ui.button("Edit Draft").clicked() {
            state.show_modal_persona_edit = !state.show_modal_persona_edit;
        }
    });

    if state.show_persona_default {
        ui.add_space(8.0);
        section(ui, "Default Persona Content");
        ui.colored_label(colors::TEXT_MUTED, state.settings.persona_default_preview.clone());
    }

    // Edit modal
    if state.show_modal_persona_edit {
        egui::Window::new("Edit Persona").resizable(true).collapsible(false)
            .default_width(480.0).default_height(360.0)
            .show(ui.ctx(), |ui| {
                ui.label("Content (identity/personality/speaking_style):");
                ui.text_edit_multiline(&mut state.settings.persona_draft);
                ui.horizontal(|ui| {
                    if ui.button("Save to Backend").clicked() {
                        let draft = state.settings.persona_draft.clone();
                        state.api_commands.push(ApiCommand::UpdatePersona { draft });
                        state.show_modal_persona_edit = false;
                        state.set_status("Saving persona to backend...".to_string());
                    }
                    if ui.button("Save (in-memory only)").clicked() {
                        state.settings.persona_modified = true;
                        state.show_modal_persona_edit = false;
                        state.set_status("Persona updated (in-memory only)".to_string());
                    }
                    if ui.button("Cancel").clicked() { state.show_modal_persona_edit = false; }
                });
            });
    }

    // Strictness/Warmth/Verbosity sliders (local overrides — backend has no numeric fields)
    ui.add_space(8.0);
    section(ui, "Persona Parameters (local overrides)");
    ui.add(egui::Slider::new(&mut state.settings.persona_warmth, 0.0..=1.0).text("Warmth"));
    ui.add(egui::Slider::new(&mut state.settings.persona_verbosity, 0.0..=1.0).text("Verbosity"));
    ui.add(egui::Slider::new(&mut state.settings.persona_strictness, 0.0..=1.0).text("Strictness"));
    ui.colored_label(colors::TEXT_MUTED, "These sliders are local-only overrides; backend persona uses text fields.");
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.colored_label(colors::TEXT_SECONDARY, title);
}