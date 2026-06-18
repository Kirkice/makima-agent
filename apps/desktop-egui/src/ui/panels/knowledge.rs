use crate::state::app_state::{ApiCommand, AppState};
use crate::theme::colors;
use eframe::egui;

/// Knowledge/RAG management panel (Phase 2)
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.colored_label(colors::RED_ACCENT, "Knowledge / RAG");
    ui.separator();
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        if ui.button("Refresh").clicked() { state.api_commands.push(ApiCommand::FetchDocuments); }
        ui.add_space(4.0);
        ui.colored_label(colors::TEXT_SECONDARY, "Retrieval");
        ui.add(egui::TextEdit::singleline(&mut state.knowledge_query).hint_text("Test query...").desired_width(180.0));
        if ui.button("Retrieve").clicked() {
            let q = state.knowledge_query.clone();
            state.api_commands.push(ApiCommand::RetrieveKnowledge(q));
        }
    });

    ui.add_space(8.0);
    ui.colored_label(colors::TEXT_SECONDARY, "Documents");
    let docs = state.settings.knowledge_docs.clone();
    if docs.is_empty() {
        ui.colored_label(colors::TEXT_MUTED, "No documents loaded. Click Refresh.");
    } else {
        for doc in &docs {
            egui::Frame::none().fill(colors::GRAPHITE_ELEVATED).rounding(egui::Rounding::same(4.0))
                .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                .show(ui, |ui| {
                    ui.label(format!("{} ({} chunks)", doc.filename.as_deref().unwrap_or("?"), doc.chunk_count.unwrap_or(0)));
                });
            ui.add_space(2.0);
        }
    }

    // Retrieval results
    let results = state.settings.knowledge_results.clone();
    if !results.is_empty() {
        ui.add_space(8.0);
        ui.colored_label(colors::TEXT_SECONDARY, "Results");
        for r in &results {
            egui::Frame::none().fill(colors::GRAPHITE_ELEVATED).rounding(egui::Rounding::same(4.0))
                .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                .show(ui, |ui| { ui.label(r); });
            ui.add_space(2.0);
        }
    }
}