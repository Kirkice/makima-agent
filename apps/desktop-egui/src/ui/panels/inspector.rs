use eframe::egui::{self, Rounding};

use crate::state::app_state::AppState;
use crate::theme::colors;

/// Draw the right inspector panel with mode, token estimate, memory status, etc.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Frame::none()
        .fill(colors::GRAPHITE_SURFACE)
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_source("inspector_panel")
                .show(ui, |ui| {
                    // ---- Mode Section ----
                    section_header(ui, "Mode");
                    if let Some(mode) = state.settings.active_mode() {
                        ui.colored_label(colors::TEXT_PRIMARY, &mode.name);
                        if let Some(desc) = &mode.description {
                            ui.colored_label(colors::TEXT_MUTED, desc);
                        }
                        ui.colored_label(
                            colors::TEXT_MUTED,
                            format!("slug: {}", mode.slug),
                        );
                    } else {
                        ui.colored_label(colors::TEXT_MUTED, "No mode selected");
                    }
                    ui.add_space(8.0);

                    // Mode selector dropdown
                    let current_mode = state
                        .settings
                        .active_mode_slug
                        .clone()
                        .unwrap_or_default();
                    egui::ComboBox::from_id_source("mode_selector")
                        .selected_text(
                            state
                                .settings
                                .active_mode()
                                .map(|m| &m.name[..])
                                .unwrap_or("Select mode"),
                        )
                        .show_ui(ui, |ui| {
                            for mode in &state.settings.modes {
                                let selected = Some(mode.slug.clone()) == state.settings.active_mode_slug;
                                if ui
                                    .selectable_label(selected, format!("{} — {}", mode.name, mode.slug))
                                    .clicked()
                                {
                                    state.settings.active_mode_slug = Some(mode.slug.clone());
                                }
                            }
                        });

                    ui.separator();
                    ui.add_space(8.0);

                    // ---- Token / Cost Section ----
                    section_header(ui, "Token & Cost");
                    if let Some(session) = state.chat.active_session() {
                        let tokens = session.estimated_token_count();
                        let cost = session.estimated_cost(state.settings.token_estimate_per_1k);

                        metric_row(ui, "Session Tokens", &format!("{}", tokens));
                        metric_row(ui, "Session Cost", &format!("${:.5}", cost));

                        // Total across all sessions
                        let total_tokens: u64 = state
                            .chat
                            .sessions
                            .iter()
                            .map(|s| s.estimated_token_count())
                            .sum();
                        let total_cost = (total_tokens as f64 / 1000.0) * state.settings.token_estimate_per_1k;

                        metric_row(ui, "Total Tokens", &format!("{}", total_tokens));
                        metric_row(ui, "Total Cost", &format!("${:.5}", total_cost));
                    }

                    ui.separator();
                    ui.add_space(8.0);

                    // ---- Model Config Section ----
                    section_header(ui, "Model");
                    let model = &state.settings.model_config;
                    metric_row(
                        ui,
                        "Provider",
                        if model.provider_configured {
                            &model.provider
                        } else {
                            "Not configured"
                        },
                    );
                    metric_row(ui, "Model", &model.model);
                    metric_row(
                        ui,
                        "Temperature",
                        &format!("{:.2}", model.temperature),
                    );

                    ui.separator();
                    ui.add_space(8.0);

                    // ---- Persona Section ----
                    section_header(ui, "Persona");
                    ui.colored_label(colors::TEXT_MUTED, "Default persona active");
                    if ui.button("Reload Persona").clicked() {
                        state.set_status("Reloading persona...".to_string());
                    }

                    ui.separator();
                    ui.add_space(8.0);

                    // ---- Memory Section ----
                    section_header(ui, "Memory");
                    metric_row(ui, "Status", "Available");
                    if ui.button("View Memories").clicked() {
                        state.set_status("Memory panel coming in Phase 2".to_string());
                    }

                    ui.separator();
                    ui.add_space(8.0);

                    // ---- MCP Section ----
                    section_header(ui, "MCP Servers");
                    let connected = state
                        .settings
                        .mcp_servers
                        .iter()
                        .filter(|s| matches!(s.status, crate::state::settings_state::McpConnectionStatus::Connected))
                        .count();
                    let total = state.settings.mcp_servers.len();

                    metric_row(ui, "Connected", &format!("{}/{}", connected, total));

                    if total == 0 {
                        ui.colored_label(colors::TEXT_MUTED, "No MCP servers configured");
                    }

                    // ---- Voice Section ----
                    ui.separator();
                    ui.add_space(8.0);
                    section_header(ui, "Voice");
                    metric_row(ui, "TTS", &state.settings.voice_config.tts_provider);
                    if state.settings.voice_config.active_voice_id.is_some() {
                        metric_row(ui, "Active Voice", "Configured");
                    } else {
                        metric_row(ui, "Active Voice", "None");
                    }
                });
        });
}

fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.colored_label(colors::RED_ACCENT, title);
}

fn metric_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(colors::TEXT_SECONDARY, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.colored_label(colors::TEXT_PRIMARY, value);
        });
    });
}