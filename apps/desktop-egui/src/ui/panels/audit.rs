use crate::state::app_state::{ApiCommand, AppState};
use crate::theme::colors;
use eframe::egui;

/// Audit panel — wired to backend via ApiCommand channel.
/// Renders real audit entries from state.settings.audit_entries.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.colored_label(colors::RED_ACCENT, "Audit Log");
    ui.separator();
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        if ui.button("Refresh").clicked() {
            let sev = match state.audit_severity_filter.as_str() {
                "all" => None,
                s => Some(s.to_string()),
            };
            state.api_commands.push(ApiCommand::QueryAuditLog { severity: sev });
            state.set_status("Fetching audit log...".to_string());
        }
        ui.colored_label(colors::TEXT_SECONDARY, "Filter:");
        let _ = ui.selectable_label(state.audit_severity_filter == "all", "All");
        let _ = ui.selectable_label(state.audit_severity_filter == "error", "Errors");
        let _ = ui.selectable_label(state.audit_severity_filter == "warn", "Warnings");
        let _ = ui.selectable_label(state.audit_severity_filter == "info", "Info");
    });

    ui.add_space(8.0);

    // Table header
    egui::Frame::NONE.fill(colors::GRAPHITE_ELEVATED).inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(colors::TEXT_SECONDARY, "Timestamp");
                ui.colored_label(colors::TEXT_SECONDARY, "Severity");
                ui.colored_label(colors::TEXT_SECONDARY, "Action");
                ui.colored_label(colors::TEXT_SECONDARY, "Resource");
            });
        });

    let entries = state.settings.audit_entries.clone();
    if entries.is_empty() {
        ui.add_space(8.0);
        ui.colored_label(colors::TEXT_MUTED, "No audit entries loaded (admin role required).");
    }

    let mut detail_request: Option<usize> = None;
    for (idx, entry) in entries.iter().enumerate() {
        let timestamp = entry.timestamp.as_deref().unwrap_or("?");
        let severity = entry.severity.as_deref().unwrap_or("info");
        let action = entry.action.as_deref().unwrap_or("?");
        let resource = entry.resource.as_deref().or(entry.resource_type.as_deref()).unwrap_or("-");

        let color = match severity {
            "error" => colors::ERROR,
            "warn" => colors::WARNING,
            _ => colors::INFO,
        };
        egui::Frame::NONE.fill(colors::GRAPHITE_SURFACE).corner_radius(egui::CornerRadius::same(2))
            .inner_margin(egui::Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(timestamp);
                    ui.colored_label(color, severity);
                    ui.label(action);
                    ui.label(resource);
                    if ui.button("Detail").clicked() { detail_request = Some(idx); }
                });
            });
    }

    ui.add_space(8.0);
    if ui.button("Export CSV").clicked() {
        export_csv(&entries, state);
    }

    // Detail window
    if let Some(idx) = state.audit_detail_index {
        if let Some(entry) = state.settings.audit_entries.get(idx) {
            let mut open = true;
            egui::Window::new("Audit Entry Detail").open(&mut open).resizable(false).collapsible(false)
                .show(ui.ctx(), |ui| {
                    detail_kv(ui, "ID", entry.id.as_str());
                    detail_kv(ui, "Timestamp", entry.timestamp.as_deref().unwrap_or("-"));
                    detail_kv(ui, "Severity", entry.severity.as_deref().unwrap_or("-"));
                    detail_kv(ui, "Action", entry.action.as_deref().unwrap_or("-"));
                    detail_kv(ui, "Resource Type", entry.resource_type.as_deref().unwrap_or("-"));
                    detail_kv(ui, "Resource ID", entry.resource_id.as_deref().unwrap_or("-"));
                    detail_kv(ui, "User", entry.user_email.as_deref().unwrap_or("-"));
                    detail_kv(ui, "Role", entry.user_role.as_deref().unwrap_or("-"));
                    detail_kv(ui, "IP", entry.ip_address.as_deref().unwrap_or("-"));
                    detail_kv(ui, "Request ID", entry.request_id.as_deref().unwrap_or("-"));
                    if let Some(dur) = entry.duration_ms {
                        detail_kv(ui, "Duration", &format!("{} ms", dur));
                    }
                    if let Some(err) = &entry.error_message {
                        ui.colored_label(colors::ERROR, format!("Error: {}", err));
                    }
                    if let Some(d) = &entry.details {
                        ui.colored_label(colors::TEXT_SECONDARY, "Details:");
                        ui.label(d.to_string());
                    }
                });
            if !open { state.audit_detail_index = None; }
        } else {
            state.audit_detail_index = None;
        }
    }

    if let Some(idx) = detail_request {
        state.audit_detail_index = Some(idx);
    }
}

fn detail_kv(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(colors::TEXT_SECONDARY, label);
        ui.colored_label(colors::TEXT_PRIMARY, value);
    });
}

/// Generate a CSV string from audit entries and log it (client-side export).
fn export_csv(entries: &[crate::api::audit::AuditEntry], state: &mut AppState) {
    let mut csv = String::from("timestamp,severity,action,resource_type,resource_id,user,ip,duration_ms,error\n");
    for e in entries {
        let ts = csv_escape(e.timestamp.as_deref().unwrap_or(""));
        let sev = csv_escape(e.severity.as_deref().unwrap_or(""));
        let act = csv_escape(e.action.as_deref().unwrap_or(""));
        let rt = csv_escape(e.resource_type.as_deref().unwrap_or(""));
        let rid = csv_escape(e.resource_id.as_deref().unwrap_or(""));
        let usr = csv_escape(e.user_email.as_deref().unwrap_or(""));
        let ip = csv_escape(e.ip_address.as_deref().unwrap_or(""));
        let dur = e.duration_ms.map(|d| d.to_string()).unwrap_or_default();
        let err = csv_escape(e.error_message.as_deref().unwrap_or(""));
        csv.push_str(&format!("{},{},{},{},{},{},{},{},{}\n", ts, sev, act, rt, rid, usr, ip, dur, err));
    }
    state.set_status(format!("CSV exported ({} rows)", entries.len()));
    tracing::info!("Audit CSV export:\n{}", csv);
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}