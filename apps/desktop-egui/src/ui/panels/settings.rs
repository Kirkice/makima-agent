use eframe::egui::{self, CornerRadius};

use crate::state::app_state::{ApiCommand, AppState, SettingsTab};
use crate::theme::colors;

use super::{audit, diagnostics, knowledge, mcp, memory, model_config, modes, persona, voice};

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.vertical(|ui| {
        // Header
        ui.colored_label(
            colors::TEXT_PRIMARY,
            egui::RichText::new("Settings").size(16.0).strong(),
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new("Agent configuration and services")
                    .color(colors::TEXT_MUTED),
            )
            .wrap(),
        );

        let sep_rect = egui::Rect::from_min_size(
            ui.cursor().min + egui::vec2(0.0, 4.0),
            egui::vec2(ui.available_width(), 1.0),
        );
        ui.painter()
            .rect_filled(sep_rect, CornerRadius::ZERO, colors::BORDER_WEAK);
        ui.add_space(8.0);

        // Tab selector dropdown
        tab_selector(ui, state);
        ui.add_space(8.0);

        // Content area
        egui::ScrollArea::vertical()
            .id_salt("settings_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                match state.settings_tab {
                    SettingsTab::Providers => {
                        // Fetch model profiles on first view
                        if state.settings.model_profiles.is_empty() {
                            state
                                .api_commands
                                .push(ApiCommand::FetchModelProfiles);
                            state
                                .api_commands
                                .push(ApiCommand::FetchProviderTypes);
                        }
                        model_config::draw(ui, state);
                    }
                    SettingsTab::Modes => {
                        if state.settings.modes.is_empty() {
                            state.api_commands.push(ApiCommand::FetchModes);
                        }
                        modes::draw(ui, state);
                    }
                    SettingsTab::Persona => {
                        persona::draw(ui, state);
                    }
                    SettingsTab::Memory => {
                        if state.settings.memory_items.is_empty() {
                            state.api_commands.push(ApiCommand::FetchMemories);
                        }
                        memory::draw(ui, state);
                    }
                    SettingsTab::Knowledge => {
                        if state.settings.knowledge_docs.is_empty() {
                            state.api_commands.push(ApiCommand::FetchDocuments);
                        }
                        knowledge::draw(ui, state);
                    }
                    SettingsTab::Mcp => {
                        if state.settings.mcp_servers.is_empty() {
                            state.api_commands.push(ApiCommand::FetchMcpServers);
                        }
                        mcp::draw(ui, state);
                    }
                    SettingsTab::Voice => {
                        voice::draw(ui, state);
                    }
                    SettingsTab::Diagnostics => {
                        diagnostics::draw(ui, state);
                    }
                    SettingsTab::Audit => {
                        if state.settings.audit_entries.is_empty() {
                            state.api_commands.push(ApiCommand::FetchAuditLog);
                        }
                        audit::draw(ui, state);
                    }
                }
            });
    });
}

fn tab_selector(ui: &mut egui::Ui, state: &mut AppState) {
    let tabs = [
        ("🔌 Providers", SettingsTab::Providers),
        ("🤖 Modes", SettingsTab::Modes),
        ("🎭 Persona", SettingsTab::Persona),
        ("🧠 Memory", SettingsTab::Memory),
        ("📚 Knowledge", SettingsTab::Knowledge),
        ("🔗 MCP", SettingsTab::Mcp),
        ("🎤 Voice", SettingsTab::Voice),
        ("🩺 Diagnostics", SettingsTab::Diagnostics),
        ("📋 Audit", SettingsTab::Audit),
    ];

    let current_label = tabs
        .iter()
        .find(|(_, t)| *t == state.settings_tab)
        .map(|(label, _)| *label)
        .unwrap_or("🔌 Providers");

    egui::ComboBox::from_id_salt("settings_tab_selector")
        .selected_text(current_label)
        .show_ui(ui, |ui| {
            for (label, tab) in &tabs {
                if ui
                    .selectable_label(state.settings_tab == *tab, *label)
                    .clicked()
                {
                    state.settings_tab = *tab;
                }
            }
        });
}