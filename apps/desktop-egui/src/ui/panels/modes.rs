use crate::state::app_state::{ApiCommand, AppState};
use crate::state::settings_state::ModeConfig;
use crate::theme::colors;
use eframe::egui::{self, CornerRadius};

/// Zoo-Code-style Mode management panel.
///
/// Features:
/// - List all modes with status indicators (active / builtin / custom)
/// - Create custom mode dialog with full field config
/// - Delete custom modes (builtin modes cannot be deleted)
/// - Reload modes from YAML config
/// - Select active mode
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.colored_label(
        colors::RED_ACCENT,
        egui::RichText::new("🛠️ Mode Management").size(15.0).strong(),
    );
    ui.add_space(8.0);

    // ── Active mode badge ────────────────────────────────────────
    if let Some(active) = state.settings.active_mode() {
        egui::Frame::NONE
            .fill(colors::RED_DIM)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(colors::RED_ACCENT, "● Active:");
                    ui.colored_label(colors::TEXT_PRIMARY, compact_emoji_name(&active.name));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.colored_label(
                            colors::TEXT_MUTED,
                            format!(
                                "temp: {:.1}  max: {} steps  tools: {}",
                                active.temperature,
                                active.max_steps,
                                active.tool_groups.len()
                            ),
                        );
                    });
                });
            });
        ui.add_space(8.0);
    }

    // ── Toolbar ───────────────────────────────────────────────────
    ui.horizontal(|ui| {
        egui::ScrollArea::horizontal()
            .id_salt("mode_toolbar")
            .show(ui, |ui| {
                if ui.button("🔄 Reload from Config").clicked() {
                    state.api_commands.push(ApiCommand::ReloadModes);
                }
                if ui.button("＋ New Mode").clicked() {
                    state.show_modal_mode_create = true;
                }
                if ui.button("📋 Refresh List").clicked() {
                    state.api_commands.push(ApiCommand::FetchModes);
                }
            });
    });
    ui.add_space(8.0);

    // ── Mode list ─────────────────────────────────────────────────
    egui::ScrollArea::vertical()
        .id_salt("mode_list")
        .auto_shrink([false, false])
        .max_height(ui.available_height().max(200.0))
        .show(ui, |ui| {
            let modes = state.settings.modes.clone();
            let total = modes.len();
            for (i, mode) in modes.iter().enumerate() {
                draw_mode_card(ui, state, mode);

                // Divider between cards
                if i < total - 1 {
                    ui.add_space(4.0);
                }
            }
        });

    // ── Create Mode Dialog ────────────────────────────────────────
    if state.show_modal_mode_create {
        draw_create_mode_dialog(ui.ctx(), state);
    }
}

// ── Mode Card ─────────────────────────────────────────────────────────

fn draw_mode_card(ui: &mut egui::Ui, state: &mut AppState, mode: &ModeConfig) {
    let is_active = Some(&mode.slug) == state.settings.active_mode_slug.as_ref();
    let is_builtin = mode.source.as_deref() == Some("builtin");
    let source_label = mode.source.as_deref().unwrap_or("custom");

    let bg = if is_active {
        colors::RED_DIM
    } else if is_builtin {
        colors::ELEVATED
    } else {
        colors::SURFACE
    };

    egui::Frame::NONE
        .fill(bg)
        .stroke(if is_active {
            egui::Stroke::new(1.0, colors::RED_ACCENT)
        } else {
            egui::Stroke::new(1.0, colors::BORDER_WEAK)
        })
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Left: icon + name + meta
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if is_active {
                            ui.colored_label(colors::RED_ACCENT, "●");
                        }
                        ui.colored_label(
                            colors::TEXT_PRIMARY,
                            egui::RichText::new(compact_emoji_name(&mode.name)).size(13.0).strong(),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.colored_label(colors::TEXT_MUTED, format!("slug: {}", mode.slug));
                        ui.add_space(8.0);
                        ui.colored_label(
                            colors::TEXT_MUTED,
                            format!(
                                "source: {}  ·  temp: {:.1}  ·  max_steps: {}",
                                source_label, mode.temperature, mode.max_steps
                            ),
                        );
                    });
                    // Tool groups
                    let tools: Vec<String> = mode.tool_groups.iter().map(|tg| tg.group.clone()).collect();
                    if !tools.is_empty() {
                        ui.colored_label(
                            colors::TEXT_MUTED,
                            format!("tools: {}", tools.join(", ")),
                        );
                    }
                    // Role definition preview (first 100 chars)
                    let preview: String = mode.role_definition.chars().take(100).collect();
                    let dots = if mode.role_definition.chars().count() > 100 { "…" } else { "" };
                    ui.colored_label(
                        colors::TEXT_MUTED,
                        egui::RichText::new(format!("role: {}{}", preview, dots)).size(11.0),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.vertical(|ui| {
                        if !is_active && ui.small_button("Select").clicked() {
                            state.settings.active_mode_slug = Some(mode.slug.clone());
                            state.set_status(format!("Switched to {}", compact_emoji_name(&mode.name)));
                        }
                        if is_active {
                            ui.add_enabled_ui(false, |ui| {
                                ui.small_button("Active");
                            });
                        }
                        // Only allow delete for custom modes
                        if !is_builtin {
                            if ui.small_button("🗑").on_hover_text("Delete custom mode").clicked() {
                                state.api_commands.push(ApiCommand::DeleteMode(mode.slug.clone()));
                            }
                        } else {
                            ui.add_enabled_ui(false, |ui| {
                                ui.small_button("🔒");
                            });
                        }
                    });
                });
            });
        });
}

// ── Create Mode Dialog ────────────────────────────────────────────────

/// Remove the space right after an emoji so names like "🛠️ Code" → "🛠️Code"
fn compact_emoji_name(name: &str) -> String {
    if let Some(idx) = name.find(|c: char| c.is_alphabetic()) {
        if idx > 0 {
            return format!("{}{}", name[..idx].trim_end(), &name[idx..]);
        }
    }
    name.to_string()
}

fn draw_create_mode_dialog(ctx: &egui::Context, state: &mut AppState) {
    egui::Window::new("＋ Create Custom Mode")
        .collapsible(false)
        .resizable(true)
        .default_width(420.0)
        .show(ctx, |ui| {
            // Store temporary form state in a separate struct
            // For simplicity, use local bool + strings
            let mut slug = String::new();
            let mut name = String::new();
            let mut role_def = String::new();
            let mut description = String::new();
            let mut when = String::new();
            let mut instructions = String::new();
            let mut tool_groups: Vec<String> = Vec::new();
            let mut max_steps: i32 = 30;
            let mut temperature: f64 = 0.0;

            // We use a simple approach: store form state in state's draft fields
            // or use immediate UI pattern with mutable locals.
            // For now, we collect input via TextEdit and assemble on Submit.

            egui::ScrollArea::vertical()
                .max_height(500.0)
                .show(ui, |ui| {
                    ui.label("Slug (unique identifier, e.g. 'my-custom-mode')");
                    let mut slug_buf = state.settings.persona_draft.clone();
                    if ui.text_edit_singleline(&mut slug_buf).changed() {
                        state.settings.persona_draft = slug_buf.clone();
                    }
                    slug = slug_buf;
                    ui.add_space(4.0);

                    ui.label("Display Name (e.g. '🎨 Creative')");
                    let mut name_buf = String::new(); // temporary
                    ui.text_edit_singleline(&mut name_buf);
                    name = name_buf;
                    ui.add_space(4.0);

                    ui.label("Role Definition (system prompt)");
                    let mut role_buf = String::new();
                    ui.text_edit_multiline(&mut role_buf);
                    role_def = role_buf;
                    ui.add_space(4.0);

                    ui.label("Description (optional)");
                    let mut desc_buf = String::new();
                    ui.text_edit_singleline(&mut desc_buf);
                    description = desc_buf;
                    ui.add_space(4.0);

                    ui.label("When to Use (optional)");
                    let mut when_buf = String::new();
                    ui.text_edit_singleline(&mut when_buf);
                    when = when_buf;
                    ui.add_space(4.0);

                    ui.label("Custom Instructions (optional)");
                    let mut instr_buf = String::new();
                    ui.text_edit_multiline(&mut instr_buf);
                    instructions = instr_buf;
                    ui.add_space(4.0);

                    // Tool groups selection
                    ui.label("Tool Groups");
                    let available_tools = ["read", "write", "command", "network", "mcp", "system"];
                    ui.horizontal_wrapped(|ui| {
                        for tool in &available_tools {
                            let mut selected = tool_groups.contains(&tool.to_string());
                            if ui.selectable_label(selected, *tool).clicked() {
                                if selected {
                                    tool_groups.retain(|t| t != tool);
                                } else {
                                    tool_groups.push(tool.to_string());
                                }
                            }
                        }
                    });
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label("Max Steps:");
                        ui.add(egui::DragValue::new(&mut max_steps).range(1..=100));
                        ui.add_space(16.0);
                        ui.label("Temperature:");
                        ui.add(egui::DragValue::new(&mut temperature).range(0.0..=2.0).speed(0.05));
                    });
                });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    state.show_modal_mode_create = false;
                }
                if ui
                    .add_sized(
                        [120.0, 28.0],
                        egui::Button::new("＋ Create")
                            .fill(colors::RED_ACCENT)
                            .stroke(egui::Stroke::NONE),
                    )
                    .clicked()
                    && !slug.is_empty()
                    && !name.is_empty()
                    && !role_def.is_empty()
                {
                    state.api_commands.push(ApiCommand::CreateMode {
                        slug: slug.trim().to_string(),
                        name: name.trim().to_string(),
                        role_definition: role_def.trim().to_string(),
                        when_to_use: if when.is_empty() { None } else { Some(when.trim().to_string()) },
                        description: if description.is_empty() { None } else { Some(description.trim().to_string()) },
                        custom_instructions: if instructions.is_empty() { None } else { Some(instructions.trim().to_string()) },
                        tool_groups,
                        max_steps,
                        temperature,
                    });
                    state.show_modal_mode_create = false;
                    state.set_status("Creating mode...".to_string());
                }
            });
        });
}