use eframe::egui::{self, CornerRadius, Sense, Vec2};

use crate::state::app_state::{ApiCommand, AppState, SettingsTab};
use crate::theme::colors;

use super::{audit, diagnostics, knowledge, mcp, memory, model_config, modes, persona, voice};

const TAB_WIDTH: f32 = 120.0;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    // ── Header ──────────────────────────────────────────────
    ui.vertical(|ui| {
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
    });

    // ── Left tab bar + Right content ────────────────────────
    let total_width = ui.available_width();
    let tab_width = TAB_WIDTH.min(total_width * 0.35);
    let content_width = total_width - tab_width - 8.0;

    ui.horizontal(|ui| {
        // Left: vertical tab list
        ui.vertical(|ui| {
            ui.set_width(tab_width);
            ui.set_min_width(tab_width);
            draw_tab_list(ui, state, tab_width);
        });

        ui.add_space(4.0);

        // Right: scrollable content
        ui.vertical(|ui| {
            ui.set_width(content_width);
            egui::ScrollArea::vertical()
                .id_salt("settings_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(content_width);
                    draw_content(ui, state);
                });
        });
    });
}

fn draw_tab_list(ui: &mut egui::Ui, state: &mut AppState, width: f32) {
    let tabs: &[(&str, SettingsTab)] = &[
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

    for (label, tab) in tabs {
        let selected = state.settings_tab == *tab;

        // Tab row
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(width, 36.0),
            Sense::click(),
        );

        // Background
        if selected || response.hovered() {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(6),
                if selected {
                    colors::SELECTION_SOFT
                } else {
                    colors::ELEVATED
                },
            );
        }

        // Left accent bar when selected
        if selected {
            let bar = egui::Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height()));
            ui.painter()
                .rect_filled(bar, CornerRadius::same(2), colors::RED_ACCENT);
        }

        // Label
        let inner = rect.shrink2(Vec2::new(12.0, 6.0));
        let text_color = if selected {
            colors::TEXT_PRIMARY
        } else if response.hovered() {
            colors::TEXT_SECONDARY
        } else {
            colors::TEXT_MUTED
        };

        // Extract text after emoji (e.g. "🔌 Providers" → "Providers")
        let display_label = strip_emoji_prefix(label);

        ui.painter().text(
            egui::pos2(inner.left() + 16.0, inner.center().y - 6.0),
            egui::Align2::LEFT_CENTER,
            display_label,
            egui::FontId::proportional(12.0),
            text_color,
        );

        // Emoji only (left of text)
        let emoji = get_emoji(label);
        ui.painter().text(
            egui::pos2(inner.left() + 2.0, inner.center().y - 6.0),
            egui::Align2::LEFT_CENTER,
            emoji,
            egui::FontId::proportional(12.0),
            text_color,
        );

        if response.clicked() {
            state.settings_tab = *tab;
        }

        ui.add_space(2.0);
    }
}

fn draw_content(ui: &mut egui::Ui, state: &mut AppState) {
    match state.settings_tab {
        SettingsTab::Providers => {
            if state.settings.model_profiles.is_empty() {
                state.api_commands.push(ApiCommand::FetchModelProfiles);
                state.api_commands.push(ApiCommand::FetchProviderTypes);
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
}

fn get_emoji(label: &str) -> &str {
    let idx = label
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(label.len());
    &label[..idx]
}

fn strip_emoji_prefix(label: &str) -> &str {
    let idx = label
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(0);
    &label[idx..]
}