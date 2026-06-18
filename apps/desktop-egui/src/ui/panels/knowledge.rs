use crate::state::app_state::AppState;
use crate::theme::colors;
use eframe::egui;

/// Knowledge/RAG management panel (Phase 2)
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.colored_label(colors::RED_ACCENT, "Knowledge / RAG");
    ui.separator();
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.colored_label(colors::TEXT_SECONDARY, "Retrieval Test");
        ui.add(egui::TextEdit::singleline(&mut state.knowledge_query).hint_text("Test query...").desired_width(200.0));
        if ui.button("Retrieve").clicked() {
            state.set_status("Retrieving knowledge...".to_string());
        }
    });

    ui.add_space(8.0);
    ui.colored_label(colors::TEXT_SECONDARY, "Documents");

    let docs: Vec<&str> = vec!["No documents uploaded", "Use backend knowledge API to manage"];
    for d in &docs {
        egui::Frame::none().fill(colors::GRAPHITE_ELEVATED).rounding(egui::Rounding::same(4.0))
            .inner_margin(egui::Margin::symmetric(8.0, 4.0))
            .show(ui, |ui| {
                ui.label(*d);
            });
        ui.add_space(2.0);
    }

    ui.add_space(8.0);
    if ui.button("Upload Document").clicked() {
        state.set_status("File upload coming in Phase 3".to_string());
    }
}