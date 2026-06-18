use crate::state::app_state::AppState;
use crate::theme::colors;
use eframe::egui;

/// Memory management panel (Phase 2)
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.colored_label(colors::RED_ACCENT, "Memory");
    ui.separator();
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.colored_label(colors::TEXT_SECONDARY, "Search");
        ui.add(egui::TextEdit::singleline(&mut state.memory_search_query).hint_text("Search memories...").desired_width(200.0));
        if ui.button("Query").clicked() {
            state.set_status("Searching memories...".to_string());
        }
    });

    ui.add_space(8.0);

    // Display memory items (placeholder)
    let memories: Vec<&str> = vec!["Memory service status: Available", "Use the backend memory API to list items"];
    let filtered: Vec<&&str> = memories.iter().filter(|m| {
        state.memory_search_query.is_empty() || m.to_lowercase().contains(&state.memory_search_query.to_lowercase())
    }).collect();

    if filtered.is_empty() {
        ui.colored_label(colors::TEXT_MUTED, "No memories found");
    } else {
        for (i, m) in filtered.iter().enumerate() {
            egui::Frame::none().fill(colors::GRAPHITE_ELEVATED).rounding(egui::Rounding::same(4.0))
                .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                .show(ui, |ui| {
                    ui.label(**m);
                });
            ui.add_space(2.0);
        }
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Pin").clicked() { state.set_status("Marking as wrong".to_string()); }
        if ui.button("Mark Wrong").clicked() { state.set_status("Marking as wrong".to_string()); }
    });
}