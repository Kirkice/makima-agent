use crate::state::app_state::{ApiCommand, AppState};
use crate::state::settings_state::McpConnectionStatus;
use crate::theme::colors;
use eframe::egui;

/// MCP management panel — wired to backend via ApiCommand channel.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.colored_label(colors::RED_ACCENT, "MCP Servers");
    ui.separator();
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        if ui.button("Refresh").clicked() {
            state.api_commands.push(ApiCommand::FetchMcpServers);
            state.set_status("Refreshing MCP servers...".to_string());
        }
        if ui.button("Add Server").clicked() {
            state.set_status("Add Server: configure via backend .makima/mcp.yaml".to_string());
        }
    });
    ui.add_space(8.0);

    if state.settings.mcp_servers.is_empty() {
        ui.colored_label(colors::TEXT_MUTED, "No MCP servers configured");
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

    // Push real ApiCommands (executed next frame by app.rs)
    if let Some(name) = reconnect_request {
        state.api_commands.push(ApiCommand::ReconnectMcp(name.clone()));
        state.set_status(format!("Reconnecting {}...", name));
    }
    if let Some((name, target)) = toggle_request {
        state.api_commands.push(ApiCommand::ToggleMcp(name.clone(), target));
        // Optimistically update local state; backend response will confirm
        if let Some(srv) = state.settings.mcp_servers.iter_mut().find(|s| s.name == name) {
            srv.enabled = target;
        }
        state.set_status(format!("{} {}...", if target { "Enabling" } else { "Disabling" }, name));
    }
}