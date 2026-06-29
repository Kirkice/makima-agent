use crate::api::marketplace::MarketplaceItem;
use crate::state::app_state::{ApiCommand, AppState};
use crate::theme::colors;
use eframe::egui::{self, CornerRadius, TextEdit};
use std::collections::HashMap;

/// MCP Marketplace panel — browse, install, and uninstall MCP servers.
/// Mirrors the layout fix from model_config.rs: no inner scroll area,
/// controls expand to fill available width.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.set_width(ui.available_width());

    // ── Header ───────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.colored_label(
            colors::TEXT_PRIMARY,
            egui::RichText::new("🛒 MCP Marketplace").size(16.0).strong(),
        );
    });
    ui.add_space(4.0);
    ui.colored_label(
        colors::TEXT_MUTED,
        egui::RichText::new("Browse, install, and manage MCP servers from the marketplace.")
            .size(11.0),
    );
    ui.add_space(12.0);

    // ── Toolbar ──────────────────────────────────────────────
    ui.horizontal(|ui| {
        if ui.button("Refresh").clicked() {
            state.settings.marketplace_loading = true;
            state.api_commands.push(ApiCommand::FetchMarketplaceItems {
                search: if state.settings.marketplace_search.is_empty() {
                    None
                } else {
                    Some(state.settings.marketplace_search.clone())
                },
                tags: if state.settings.marketplace_selected_tags.is_empty() {
                    None
                } else {
                    Some(state.settings.marketplace_selected_tags.clone())
                },
            });
            state.set_status("Refreshing marketplace...".to_string());
        }
        if ui.button("Fetch Tags").clicked() {
            state.api_commands.push(ApiCommand::FetchMarketplaceTags);
        }
    });
    ui.add_space(4.0);

    // ── Search ───────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label("Search:");
        let search_changed = ui
            .add(
                TextEdit::singleline(&mut state.settings.marketplace_search)
                    .desired_width(ui.available_width() - 80.0)
                    .hint_text("Filter by name or description..."),
            )
            .changed();
        if search_changed {
            // Debounced search could be added here
        }
        if ui.button("Search").clicked() {
            state.settings.marketplace_loading = true;
            state.api_commands.push(ApiCommand::FetchMarketplaceItems {
                search: if state.settings.marketplace_search.is_empty() {
                    None
                } else {
                    Some(state.settings.marketplace_search.clone())
                },
                tags: if state.settings.marketplace_selected_tags.is_empty() {
                    None
                } else {
                    Some(state.settings.marketplace_selected_tags.clone())
                },
            });
        }
    });
    ui.add_space(4.0);

    // ── Tag filters ──────────────────────────────────────────
    if !state.settings.marketplace_tags.is_empty() {
        ui.label(egui::RichText::new("Tags:").size(11.0).color(colors::TEXT_SECONDARY));
        let tags = state.settings.marketplace_tags.clone();
        let selected = state.settings.marketplace_selected_tags.clone();

        ui.horizontal_wrapped(|ui| {
            for tag in &tags {
                let is_selected = selected.contains(tag);
                let btn = egui::Button::new(egui::RichText::new(tag).size(10.0))
                    .small()
                    .fill(if is_selected {
                        colors::RED_ACCENT.linear_multiply(0.3)
                    } else {
                        colors::SURFACE
                    });
                if ui.add(btn).clicked() {
                    if is_selected {
                        state
                            .settings
                            .marketplace_selected_tags
                            .retain(|t| t != tag);
                    } else {
                        state.settings.marketplace_selected_tags.push(tag.clone());
                    }
                }
            }
        });
        ui.add_space(4.0);
    }

    // ── Items list ───────────────────────────────────────────
    // NOTE: No inner ScrollArea — the outer Settings scroll handles scrolling.
    if state.settings.marketplace_loading {
        ui.colored_label(colors::WARNING, "Loading marketplace items...");
    } else if state.settings.marketplace_items.is_empty() {
        ui.colored_label(colors::TEXT_MUTED, "No marketplace items loaded. Click Refresh to load.");
    } else {
        ui.add_space(4.0);
        ui.colored_label(
            colors::TEXT_MUTED,
            egui::RichText::new(format!(
                "{} items available",
                state.settings.marketplace_items.len()
            ))
            .size(10.0),
        );
        ui.add_space(8.0);

        let items = state.settings.marketplace_items.clone();
        let mut install_clicked: Option<MarketplaceItem> = None;
        let mut uninstall_clicked: Option<String> = None;

        for item in &items {
            draw_marketplace_card(ui, item, &mut install_clicked, &mut uninstall_clicked);
            ui.add_space(6.0);
        }

        // Handle install click
        if let Some(item) = install_clicked {
            state.settings.marketplace_install_item = Some(item);
        }

        // Handle uninstall click
        if let Some(item_id) = uninstall_clicked {
            state
                .api_commands
                .push(ApiCommand::UninstallMarketplaceItem {
                    item_id: item_id.clone(),
                    target: "global".to_string(),
                });
            state.set_status(format!("Uninstalling {}...", item_id));
        }
    }

    // ── Install modal ────────────────────────────────────────
    if state.settings.marketplace_install_item.is_some() {
        draw_install_modal(ui, state);
    }
}

/// Draw a single marketplace item card
fn draw_marketplace_card(
    ui: &mut egui::Ui,
    item: &MarketplaceItem,
    install_clicked: &mut Option<MarketplaceItem>,
    uninstall_clicked: &mut Option<String>,
) {
    egui::Frame::NONE
        .fill(colors::GRAPHITE_ELEVATED)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .stroke(egui::Stroke::new(0.5, colors::BORDER_WEAK))
        .show(ui, |ui| {
            // Header: name + install/uninstall button
            ui.horizontal(|ui| {
                // Name (clickable if URL exists)
                if let Some(url) = &item.url {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(&item.name)
                                    .size(14.0)
                                    .strong()
                                    .color(colors::TEXT_PRIMARY),
                            )
                            .fill(colors::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                        )
                        .on_hover_text(url)
                        .clicked()
                    {
                        let _ = open::that(url);
                    }
                } else {
                    ui.label(
                        egui::RichText::new(&item.name)
                            .size(14.0)
                            .strong()
                            .color(colors::TEXT_PRIMARY),
                    );
                }

                // Author
                if let Some(author) = &item.author {
                    ui.label(
                        egui::RichText::new(format!("by {}", author))
                            .size(10.0)
                            .color(colors::TEXT_MUTED),
                    );
                }

                // Spacer
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if item.installed {
                        // Uninstall button
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Uninstall")
                                        .size(11.0)
                                        .color(colors::ERROR),
                                )
                                .small(),
                            )
                            .clicked()
                        {
                            *uninstall_clicked = Some(item.id.clone());
                        }

                        // Installed badge
                        ui.label(
                            egui::RichText::new("✓ Installed")
                                .size(10.0)
                                .color(colors::SUCCESS),
                        );
                    } else {
                        // Install button
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Install").size(11.0).color(colors::TEXT_PRIMARY),
                                )
                                .fill(colors::RED_ACCENT)
                                .stroke(egui::Stroke::NONE)
                                .small(),
                            )
                            .clicked()
                        {
                            *install_clicked = Some(item.clone());
                        }
                    }
                });
            });

            // Description
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(&item.description)
                    .size(11.0)
                    .color(colors::TEXT_SECONDARY),
            );

            // Tags
            if !item.tags.is_empty() {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for tag in &item.tags {
                        ui.label(
                            egui::RichText::new(tag)
                                .size(9.0)
                                .color(colors::TEXT_MUTED)
                                .background_color(colors::SURFACE),
                        );
                    }
                });
            }

            // Prerequisites
            if !item.prerequisites.is_empty() {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Requires: {}",
                        item.prerequisites.join(", ")
                    ))
                    .size(9.0)
                    .color(colors::TEXT_MUTED),
                );
            }
        });
}

/// Draw the install modal dialog
fn draw_install_modal(ui: &mut egui::Ui, state: &mut AppState) {
    let item = state.settings.marketplace_install_item.clone();
    if let Some(item) = item {
        let mut open = true;

        egui::Window::new(format!("Install: {}", item.name))
            .collapsible(false)
            .resizable(true)
            .default_width(400.0)
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                ui.label(egui::RichText::new(&item.description).size(11.0));
                ui.add_space(8.0);

                // Installation methods
                let methods = item.content.methods();
                if methods.len() > 1 {
                    ui.label(egui::RichText::new("Installation Method:").strong().size(12.0));
                    ui.add_space(4.0);

                    // TODO: Add method selection dropdown
                    for (idx, method) in methods.iter().enumerate() {
                        ui.radio_value(&mut 0usize, idx, &method.name);
                    }
                    ui.add_space(8.0);
                }

                // Prerequisites
                if !item.prerequisites.is_empty() {
                    ui.label(egui::RichText::new("Prerequisites:").strong().size(12.0));
                    for prereq in &item.prerequisites {
                        ui.label(egui::RichText::new(format!("• {}", prereq)).size(11.0));
                    }
                    ui.add_space(8.0);
                }

                // Parameters
                let all_params = get_all_parameters(&item);
                if !all_params.is_empty() {
                    ui.label(egui::RichText::new("Configuration:").strong().size(12.0));
                    ui.add_space(4.0);

                    // We need to store parameter values somewhere persistent
                    // For now, use a simple approach with the item's id as key
                    ui.label(
                        egui::RichText::new("Enter required parameters below:")
                            .size(10.0)
                            .color(colors::TEXT_MUTED),
                    );
                    ui.add_space(4.0);

                    // Use a simple text storage approach
                    let mut param_values: HashMap<String, String> = HashMap::new();

                    for param in &all_params {
                        ui.horizontal(|ui| {
                            let label = if param.optional {
                                format!("{} (optional):", param.name)
                            } else {
                                format!("{}*:", param.name)
                            };
                            ui.label(egui::RichText::new(label).size(11.0));

                            let mut value = param_values
                                .get(&param.key)
                                .cloned()
                                .unwrap_or_default();
                            let resp = ui.add(
                                TextEdit::singleline(&mut value)
                                    .desired_width(200.0)
                                    .hint_text(&param.placeholder),
                            );
                            if resp.changed() {
                                param_values.insert(param.key.clone(), value);
                            }
                        });
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Buttons
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        state.settings.marketplace_install_item = None;
                    }

                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Install").color(colors::TEXT_PRIMARY),
                            )
                            .fill(colors::RED_ACCENT),
                        )
                        .clicked()
                    {
                        // Collect parameter values from UI state
                        // For now, send empty parameters (user can configure via config file)
                        let parameters: HashMap<String, String> = HashMap::new();

                        state
                            .api_commands
                            .push(ApiCommand::InstallMarketplaceItem {
                                item_id: item.id.clone(),
                                target: "global".to_string(),
                                selected_method_index: None,
                                parameters,
                            });
                        state.set_status(format!("Installing {}...", item.name));
                        state.settings.marketplace_install_item = None;
                    }
                });
            });

        if !open {
            state.settings.marketplace_install_item = None;
        }
    }
}

/// Get all parameters (global + method-specific) for an item
fn get_all_parameters(
    item: &MarketplaceItem,
) -> Vec<crate::api::marketplace::McpParameter> {
    let mut params: Vec<crate::api::marketplace::McpParameter> = item.parameters.clone();

    // Add method-specific parameters
    let methods = item.content.methods();
    if let Some(method) = methods.first() {
        for p in &method.parameters {
            if !params.iter().any(|existing| existing.key == p.key) {
                params.push(p.clone());
            }
        }
    }

    params
}