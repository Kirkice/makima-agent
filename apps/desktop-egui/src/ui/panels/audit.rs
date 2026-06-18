use crate::state::app_state::AppState;
use crate::theme::colors;
use eframe::egui;

/// Audit/Admin panel (Phase 4)
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.colored_label(colors::RED_ACCENT, "Audit Log");
    ui.separator();
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.colored_label(colors::TEXT_SECONDARY, "Filter:");
        ui.selectable_label(state.audit_severity_filter == "all", "All");
        ui.selectable_label(state.audit_severity_filter == "error", "Errors");
        ui.selectable_label(state.audit_severity_filter == "warn", "Warnings");
        ui.selectable_label(state.audit_severity_filter == "info", "Info");
    });

    ui.add_space(8.0);

    // Placeholder table header
    egui::Frame::none().fill(colors::GRAPHITE_ELEVATED).inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(colors::TEXT_SECONDARY, "Timestamp");
                ui.colored_label(colors::TEXT_SECONDARY, "Severity");
                ui.colored_label(colors::TEXT_SECONDARY, "Action");
                ui.colored_label(colors::TEXT_SECONDARY, "Resource");
            });
        });

    // Placeholder rows
    let placeholder = vec![
        ("2025-01-01", "info", "login", "auth"),
        ("2025-01-01", "warn", "access_denied", "sessions"),
    ];

    for (ts, severity, action, resource) in &placeholder {
        let color = match *severity { "error" => colors::ERROR, "warn" => colors::WARNING, _ => colors::INFO };
        egui::Frame::none().fill(colors::GRAPHITE_SURFACE).rounding(egui::Rounding::same(2.0))
            .inner_margin(egui::Margin::symmetric(8.0, 3.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(*ts);
                    ui.colored_label(color, *severity);
                    ui.label(*action);
                    ui.label(*resource);
                    if ui.button("Detail").clicked() {}
                });
            });
    }

    ui.add_space(8.0);
    if ui.button("Export CSV").clicked() { state.set_status("Export CSV coming soon".to_string()); }
}