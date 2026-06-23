use crate::state::app_state::AppState;
use crate::state::settings_state::McpConnectionStatus;
use crate::theme::colors;
use eframe::egui;

/// MCP management panel (Phase 3)
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.colored_label(colors::RED_ACCENT, "MCP Servers");
    ui.separator();
    ui.add_space(8.0);

    if state.settings.mcp_servers.is_empty() {
        ui.colored_label(colors::TEXT_MUTED, "No MCP servers configured");
        ui.add_space(8.0);
        if ui.button("Add Server").clicked() {
            state.set_status("MCP server config coming in Phase 3".to_string());
        }
        return;
    }

    // Clone server data to avoid borrow issues
    let servers = state.settings.mcp_servers.clone();
    let mut reconnect_request: Option<String> = None;
    let mut toggle_request: Option<(String, bool)> = None;

    for server in &servers {
        let (status_color, status_text) = match server.status {
            McpConnectionStatus::Connected => (colors::SUCCESS, "Connected"),
            McpConnectionStatus::Connecting => (colors::WARNING, "Connecting..."),
            McpConnectionStatus::Disconnected => (colors::TEXT_MUTED, "Disconnected"),
            McpConnectionStatus::Error => (colors::ERROR, "Error"),
        };

        egui::Frame::NONE.fill(colors::GRAPHITE_ELEVATED).corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(status_color, format!("● {}", server.name));
                    ui.colored_label(colors::TEXT_MUTED, status_text);
                });
                ui.colored_label(colors::TEXT_MUTED, format!("Transport: {:?}", server.transport_type));

                if let Some(err) = &server.error {
                    ui.colored_label(colors::ERROR, format!("Last error: {}", err));
                }

                if !server.tools.is_empty() {
                    ui.colored_label(colors::TEXT_SECONDARY, "Tools:");
                    for tool in &server.tools {
                        ui.colored_label(colors::TEXT_MUTED, format!("  • {} ({})",
                            tool.name, if tool.enabled { "enabled" } else { "disabled" }));
                    }
                }

                let name = server.name.clone();
                ui.horizontal(|ui| {
                    if ui.button("Reconnect").clicked() { reconnect_request = Some(name.clone()); }
                    if ui.button(if server.enabled { "Disable" } else { "Enable" }).clicked() {
                        toggle_request = Some((name.clone(), !server.enabled));
                    }
                });
            });
        ui.add_space(4.0);
    }

    // Apply actions outside the loop
    if let Some(name) = reconnect_request {
        state.set_status(format!("Reconnecting {}...", name));
    }
    if let Some((name, target)) = toggle_request {
        if let Some(srv) = state.settings.mcp_servers.iter_mut().find(|s| s.name == name) {
            srv.enabled = target;
        }
    }
}