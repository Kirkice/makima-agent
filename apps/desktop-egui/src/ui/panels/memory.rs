use crate::state::app_state::AppState;
use crate::theme::colors;
use eframe::egui;

/// Memory management panel (Phase 2)
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    use crate::state::app_state::ApiCommand;

    ui.colored_label(colors::RED_ACCENT, "Memory");
    ui.separator();
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        if ui.button("Refresh").clicked() { state.api_commands.push(ApiCommand::FetchMemories); }
        ui.add_space(4.0);
        ui.colored_label(colors::TEXT_SECONDARY, "Search");
        ui.add(egui::TextEdit::singleline(&mut state.memory_search_query).hint_text("Search...").desired_width(160.0));
        if ui.button("Search").clicked() {
            let q = state.memory_search_query.clone();
            state.api_commands.push(ApiCommand::SearchMemories(q));
        }
    });

    ui.add_space(8.0);
    let items = state.settings.memory_items.clone();
    if items.is_empty() {
        ui.colored_label(colors::TEXT_MUTED, "No memories loaded. Click Refresh.");
    } else {
        for (idx, m) in items.iter().enumerate() {
            egui::Frame::none().fill(colors::GRAPHITE_ELEVATED).rounding(egui::Rounding::same(4.0))
                .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(m);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("×").clicked() {
                                state.api_commands.push(ApiCommand::DeleteMemory(format!("{}", idx)));
                            }
                        });
                    });
                });
            ui.add_space(2.0);
        }
    }
}