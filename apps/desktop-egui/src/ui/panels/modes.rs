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
    // ── Header: compact title + toolbar on same row ───────────────
    ui.horizontal(|ui| {
        ui.colored_label(
            colors::TEXT_PRIMARY,
            egui::RichText::new("Modes").size(14.0).strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("＋ New")
                .on_hover_text("Create a custom mode")
                .clicked()
            {
                state.show_modal_mode_create = true;
            }
            if ui
                .small_button("↻")
                .on_hover_text("Reload from config")
                .clicked()
            {
                state.api_commands.push(ApiCommand::ReloadModes);
            }
        });
    });

    // ── Active mode inline chip ───────────────────────────────────
    if let Some(active) = state.settings.active_mode() {
        ui.add_space(4.0);
        egui::Frame::NONE
            .fill(colors::RED_DIM)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.colored_label(colors::RED_ACCENT, "●");
                    ui.colored_label(
                        colors::TEXT_PRIMARY,
                        egui::RichText::new(compact_emoji_name(&active.name))
                            .size(12.0)
                            .strong(),
                    );
                    ui.colored_label(
                        colors::TEXT_MUTED,
                        egui::RichText::new(format!(
                            "· t{:.1} · {} steps · {} tools",
                            active.temperature,
                            active.max_steps,
                            active.tool_groups.len()
                        ))
                        .size(11.0),
                    );
                });
            });
    }

    ui.add_space(8.0);

    // ── Mode list (no nested scroll — parent settings ScrollArea handles it) ──
    let modes = state.settings.modes.clone();
    let total = modes.len();
    for (i, mode) in modes.iter().enumerate() {
        draw_mode_card(ui, state, mode);
        if i < total - 1 {
            ui.add_space(6.0);
        }
    }

    // ── Create Mode Dialog ────────────────────────────────────────
    if state.show_modal_mode_create {
        draw_create_mode_dialog(ui.ctx(), state);
    }
}

// ── Mode Card ─────────────────────────────────────────────────────────

fn draw_mode_card(ui: &mut egui::Ui, state: &mut AppState, mode: &ModeConfig) {
    let is_active = Some(&mode.slug) == state.settings.active_mode_slug.as_ref();
    let is_builtin = mode.source.as_deref() == Some("builtin");

    let bg = if is_active {
        colors::RED_DIM
    } else {
        colors::ELEVATED
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
            // ── Row 1: name + action button ────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if is_active {
                    ui.colored_label(colors::RED_ACCENT, "●");
                }
                ui.colored_label(
                    colors::TEXT_PRIMARY,
                    egui::RichText::new(compact_emoji_name(&mode.name)).size(13.0).strong(),
                );
                // Source badge (compact)
                if !is_builtin {
                    egui::Frame::NONE
                        .fill(colors::SURFACE)
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(egui::Margin::symmetric(4, 1))
                        .show(ui, |ui| {
                            ui.colored_label(
                                colors::TEXT_MUTED,
                                egui::RichText::new("custom").size(9.0),
                            );
                        });
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_active {
                        egui::Frame::NONE
                            .fill(colors::RED_ACCENT)
                            .corner_radius(CornerRadius::same(4))
                            .inner_margin(egui::Margin::symmetric(5, 2))
                            .show(ui, |ui| {
                                ui.colored_label(
                                    colors::SURFACE,
                                    egui::RichText::new("Active").size(10.0).strong(),
                                );
                            });
                    } else if ui
                        .add(egui::Button::new(
                            egui::RichText::new("Select").size(11.0),
                        ).small())
                        .clicked()
                    {
                        state.settings.active_mode_slug = Some(mode.slug.clone());
                        state.set_status(format!("Switched to {}", compact_emoji_name(&mode.name)));
                    }
                });
            });

            // ── Row 2: meta line (compact) ─────────────────────────
            ui.add_space(3.0);
            let tools: Vec<String> = mode.tool_groups.iter().map(|tg| tg.group.clone()).collect();
            let meta = format!(
                "t{:.1}  ·  {} steps{}{}",
                mode.temperature,
                mode.max_steps,
                if tools.is_empty() {
                    String::new()
                } else {
                    format!("  ·  {}", tools.join(", "))
                },
                if !is_builtin { format!("  ·  {}", mode.slug) } else { String::new() },
            );
            ui.colored_label(
                colors::TEXT_MUTED,
                egui::RichText::new(meta).size(11.0),
            );
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