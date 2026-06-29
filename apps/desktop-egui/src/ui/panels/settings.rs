use eframe::egui::{self, Align, CornerRadius, Layout, Vec2};

use crate::state::app_state::{ApiCommand, AppState, SettingsTab};
use crate::theme::colors;

use super::{audit, diagnostics, knowledge, mcp, marketplace, memory, model_config, modes, persona, voice};
use crate::ui::chat::composer::draw_auto_approval_panel;

/// Zoo-Code-inspired sidebar constants.
/// Zoo-Code uses w-48 (192px), h-12 (48px), px-4 py-3 (16px / 12px).
const SIDEBAR_WIDTH: f32 = 192.0;
const SIDEBAR_INNER_PAD_X: f32 = 16.0;
const SIDEBAR_INNER_PAD_Y: f32 = 8.0;

const TAB_ITEM_HEIGHT: f32 = 48.0;
const TAB_ITEM_RADIUS: f32 = 8.0;
const TAB_TEXT_LEFT_PAD: f32 = 18.0;
const TAB_ITEM_SPACING: f32 = 2.0;
const ACCENT_BAR_WIDTH: f32 = 2.0;
const ACCENT_BAR_V_INSET: f32 = 12.0;
const TAB_TEXT_SIZE: f32 = 13.5;
const HEADER_TEXT_SIZE: f32 = 15.0;
const CONTENT_PAD_X: f32 = 16.0;
const CONTENT_PAD_Y: f32 = 12.0;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    let available = ui.available_size_before_wrap();
    ui.set_min_size(available);

    let total_width = available.x.max(0.0);
    let total_height = available.y.max(0.0);
    let tab_width = SIDEBAR_WIDTH.min(total_width * 0.50);
    let sep_width = 1.0;
    let content_width = (total_width - tab_width - sep_width).max(0.0);

    ui.allocate_ui_with_layout(
        Vec2::new(total_width, total_height),
        Layout::left_to_right(Align::Min),
        |ui| {
            // ── Tab sidebar ───────────────────────────────────────
            ui.vertical(|ui| {
                ui.set_width(tab_width);
                ui.set_min_width(tab_width);
                ui.set_min_height(total_height);
                draw_tab_list(ui, state, tab_width, total_height);
            });

            // ── Separator line (Zoo-Code border-r) ────────────────
            let sep_x = ui.cursor().min.x;
            let sep_rect = egui::Rect::from_min_max(
                egui::pos2(sep_x, ui.cursor().min.y),
                egui::pos2(sep_x + sep_width, ui.cursor().min.y + total_height),
            );
            ui.painter()
                .rect_filled(sep_rect, CornerRadius::ZERO, colors::BORDER_WEAK);
            ui.add_space(sep_width);

            // ── Content area ──────────────────────────────────────
            ui.vertical(|ui| {
                ui.set_width(content_width);
                ui.set_min_width(content_width);
                ui.set_min_height(total_height);

                egui::ScrollArea::vertical()
                    .id_salt("settings_scroll")
                    .auto_shrink([false, false])
                    .max_height(total_height)
                    .show(ui, |ui| {
                        ui.set_width(content_width);
                        ui.add_space(CONTENT_PAD_Y);
                        ui.horizontal(|ui| {
                            ui.add_space(CONTENT_PAD_X);
                            ui.vertical(|ui| {
                                ui.set_width(content_width - CONTENT_PAD_X * 2.0);
                                draw_content(ui, state);
                            });
                        });
                    });
            });
        },
    );
}

fn draw_tab_list(ui: &mut egui::Ui, state: &mut AppState, width: f32, total_height: f32) {
    // Sidebar background fill
    let bg_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        Vec2::new(width, total_height),
    );
    ui.painter()
        .rect_filled(bg_rect, CornerRadius::ZERO, colors::SURFACE);

    // Top padding
    ui.add_space(SIDEBAR_INNER_PAD_Y);

    // ── Header ────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.add_space(SIDEBAR_INNER_PAD_X);
        ui.colored_label(
            colors::TEXT_PRIMARY,
            egui::RichText::new("Settings").size(HEADER_TEXT_SIZE).strong(),
        );
    });
    ui.add_space(12.0);

    // ── Tab items ─────────────────────────────────────────────────
    ui.spacing_mut().item_spacing.y = TAB_ITEM_SPACING;

    let tabs: &[(&str, SettingsTab)] = &[
        ("\u{1f50c} Providers", SettingsTab::Providers),
        ("\u{1f916} Modes", SettingsTab::Modes),
        ("\u{1f3ad} Persona", SettingsTab::Persona),
        ("\u{1f9e0} Memory", SettingsTab::Memory),
        ("\u{1f4da} Knowledge", SettingsTab::Knowledge),
        ("\u{1f517} MCP", SettingsTab::Mcp),
        ("\u{1f6d2} Marketplace", SettingsTab::Marketplace),
        ("\u{1f3a4} Voice", SettingsTab::Voice),
        ("\u{1fa7a} Diagnostics", SettingsTab::Diagnostics),
        ("\u{1f4cb} Audit", SettingsTab::Audit),
        ("\u{2699} Auto-Approve", SettingsTab::AutoApprove),
    ];

    let item_width = width - SIDEBAR_INNER_PAD_X * 2.0;
    for (label, tab) in tabs {
        ui.horizontal(|ui| {
            ui.add_space(SIDEBAR_INNER_PAD_X);
            let selected = state.settings_tab == *tab;
            draw_nav_item(ui, state, item_width, label, *tab, selected);
        });
    }
}

fn draw_nav_item(
    ui: &mut egui::Ui,
    state: &mut AppState,
    width: f32,
    label: &str,
    tab: SettingsTab,
    selected: bool,
) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, TAB_ITEM_HEIGHT), egui::Sense::click());

    let hovered = response.hovered();
    let radius = CornerRadius::same(TAB_ITEM_RADIUS as u8);

    // Background fill — rounded pill for selected/hover, like Zoo-Code
    if selected {
        ui.painter().rect_filled(rect, radius, colors::SELECTION_SOFT);
    } else if hovered {
        ui.painter().rect_filled(rect, radius, colors::ELEVATED);
    }

    // Left accent bar — flush with the rect left edge
    if selected {
        let bar = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.min.y + ACCENT_BAR_V_INSET),
            Vec2::new(ACCENT_BAR_WIDTH, rect.height() - ACCENT_BAR_V_INSET * 2.0),
        );
        ui.painter()
            .rect_filled(bar, CornerRadius::same(2), colors::RED_ACCENT);
    }

    // Label text
    let text_color = if selected {
        colors::TEXT_PRIMARY
    } else if hovered {
        colors::TEXT_SECONDARY
    } else {
        colors::TEXT_MUTED
    };
    let text_pos = rect.min + egui::vec2(TAB_TEXT_LEFT_PAD, 0.0);
    ui.painter().text(
        egui::pos2(text_pos.x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(TAB_TEXT_SIZE),
        text_color,
    );

    if response.clicked() {
        state.settings_tab = tab;
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
        SettingsTab::Marketplace => {
            if state.settings.marketplace_items.is_empty() && !state.settings.marketplace_loading {
                state.settings.marketplace_loading = true;
                state.api_commands.push(ApiCommand::FetchMarketplaceItems {
                    search: None,
                    tags: None,
                });
                state.api_commands.push(ApiCommand::FetchMarketplaceTags);
            }
            marketplace::draw(ui, state);
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
        SettingsTab::AutoApprove => {
            draw_auto_approval_panel(ui, state);
        }
    }
}
