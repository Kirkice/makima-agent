use crate::api::model_profiles::ModelProfile;
use crate::state::app_state::{ApiCommand, AppState};
use crate::theme::colors;
use eframe::egui::{self, CornerRadius, Frame, Margin, Stroke, RichText};

/// Model configuration panel — Zoo-Code style multi-profile management.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.set_width(ui.available_width());
        // ── Header ────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.colored_label(
                colors::TEXT_PRIMARY,
                RichText::new("⚙ Model Configuration").size(16.0).strong(),
            );
        });
        ui.add_space(4.0);
        ui.colored_label(
            colors::TEXT_MUTED,
            RichText::new("Configure model profiles to use with Makima. Each profile defines a provider, model, and connection settings.").size(11.0),
        );
        ui.add_space(12.0);

        // ── Profile selector bar ──────────────────────────────────
        draw_profile_bar(ui, state);
        ui.add_space(12.0);

        // ── Edit form or empty state ──────────────────────────────
        let mut editing = state.settings.model_profile_editing.take();
        let is_creating = state.settings.show_model_profile_create && editing.is_none();
        if is_creating {
            editing = Some(ModelProfile::default());
            state.settings.model_profile_show_api_key = false;
        }

        if let Some(ref mut profile) = editing {
            let keep_editing = draw_edit_form(ui, state, profile);
            if !keep_editing {
                editing = None;
            }
        } else if state.settings.model_profiles.is_empty() {
            draw_empty_state(ui, state);
        } else {
            draw_profile_list(ui, state);
        }

    state.settings.model_profile_editing = editing;
}

// ── Profile selector bar ─────────────────────────────────────────────

fn draw_profile_bar(ui: &mut egui::Ui, state: &mut AppState) {
    Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 8))
        .stroke(Stroke::new(1.0, colors::BORDER_WEAK))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(colors::TEXT_SECONDARY, RichText::new("Profile:").size(12.0));

                let active_name = state
                    .settings
                    .active_model_profile
                    .clone()
                    .unwrap_or_else(|| "—".to_string());

                let profiles_snapshot: Vec<(String, String, bool)> = state
                    .settings
                    .model_profiles
                    .iter()
                    .map(|p| {
                        let is_active =
                            state.settings.active_model_profile.as_deref() == Some(&p.name);
                        (p.name.clone(), p.model.clone(), is_active)
                    })
                    .collect();

                let mut activate_name: Option<String> = None;
                let combo_width = ui.available_width().clamp(180.0, 320.0);
                egui::ComboBox::from_id_salt("profile_selector")
                    .selected_text(&active_name)
                    .width(combo_width)
                    .show_ui(ui, |ui| {
                        for (name, model, is_active) in &profiles_snapshot {
                            let label = format!("{} ({})", name, model);
                            if ui.selectable_label(*is_active, &label).clicked() {
                                activate_name = Some(name.clone());
                            }
                        }
                    });
                if let Some(name) = activate_name {
                    state.api_commands.push(ApiCommand::ActivateModelProfile(name.clone()));
                    state.set_status(format!("Activating profile '{}'...", name));
                }

                ui.separator();

                if ui
                    .add(
                        egui::Button::new("+")
                            .fill(colors::RED_ACCENT)
                            .stroke(Stroke::NONE)
                            .min_size(egui::vec2(28.0, 24.0)),
                    )
                    .on_hover_text("Create new profile")
                    .clicked()
                {
                    state.settings.show_model_profile_create = true;
                    state.settings.model_profile_editing = None;
                    state.settings.model_profile_edit_name = String::new();
                }

                if let Some(active_name) = &state.settings.active_model_profile {
                    if let Some(profile) = state
                        .settings
                        .model_profiles
                        .iter()
                        .find(|p| &p.name == active_name)
                    {
                        if ui.button("✏️ Edit").clicked() {
                            state.settings.model_profile_editing = Some(profile.clone());
                            state.settings.model_profile_edit_name = profile.name.clone();
                            state.settings.model_profile_show_api_key = false;
                            state.settings.show_model_profile_create = false;
                        }
                    }
                }

                if let Some(active_name) = state.settings.active_model_profile.clone() {
                    if ui.button("🗑 Delete").on_hover_text("Delete this profile").clicked() {
                        state.api_commands.push(ApiCommand::DeleteModelProfile(active_name.clone()));
                        state.set_status(format!("Deleting profile '{}'...", active_name));
                    }
                }
            });
        });
}

// ── Empty state ──────────────────────────────────────────────────────

fn draw_empty_state(ui: &mut egui::Ui, state: &mut AppState) {
    Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(20, 30))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.colored_label(colors::TEXT_MUTED, RichText::new("🤖").size(32.0));
                ui.add_space(8.0);
                ui.colored_label(
                    colors::TEXT_SECONDARY,
                    RichText::new("No model profiles configured").size(14.0),
                );
                ui.add_space(4.0);
                ui.colored_label(colors::TEXT_MUTED, "Create your first profile to get started.");
                ui.add_space(12.0);
                if ui
                    .add(
                        egui::Button::new("+ Create your first profile")
                            .fill(colors::RED_ACCENT)
                            .stroke(Stroke::NONE)
                            .min_size(egui::vec2(200.0, 32.0)),
                    )
                    .clicked()
                {
                    state.settings.show_model_profile_create = true;
                    state.settings.model_profile_edit_name = String::new();
                }
            });
        });
}

// ── Profile list (when not editing) ──────────────────────────────────

fn draw_profile_list(ui: &mut egui::Ui, state: &mut AppState) {
    ui.colored_label(
        colors::TEXT_SECONDARY,
        RichText::new("Configured Profiles").size(12.0).strong(),
    );
    ui.add_space(4.0);

    let profiles = state.settings.model_profiles.clone();
    let active_name = state.settings.active_model_profile.clone();

    for p in &profiles {
        let is_active = active_name.as_deref() == Some(&p.name);
        let accent = if is_active { colors::SUCCESS } else { colors::TEXT_MUTED };

        let frame = Frame::NONE
            .fill(colors::ELEVATED)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(10, 6))
            .stroke(Stroke::new(
                if is_active { 1.5 } else { 0.5 },
                if is_active { colors::SUCCESS } else { colors::BORDER_WEAK },
            ));

        let response = frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(accent, if is_active { "●" } else { "○" });
                ui.vertical(|ui| {
                    ui.colored_label(
                        colors::TEXT_PRIMARY,
                        RichText::new(&p.name).size(13.0).strong(),
                    );
                    ui.colored_label(
                        colors::TEXT_MUTED,
                        RichText::new(format!("{} · {} · {}", p.provider, p.model, p.base_url)).size(11.0),
                    );
                });
            });
        });

        if response.response.clicked() {
            state.api_commands.push(ApiCommand::ActivateModelProfile(p.name.clone()));
            state.set_status(format!("Activating profile '{}'...", p.name));
        }
        ui.add_space(2.0);
    }
}

// ── Edit form (Zoo-Code style card layout) ───────────────────────────

fn draw_edit_form(ui: &mut egui::Ui, state: &mut AppState, profile: &mut ModelProfile) -> bool {
    let mut keep_editing = true;
    let field_width = ui.available_width().max(240.0);
    let api_key_width = (field_width - 40.0).max(200.0);

    // ── Profile Name ──────────────────────────────────────────────
    card_section(ui, "Profile Name", |ui| {
        let name_edit = egui::TextEdit::singleline(&mut state.settings.model_profile_edit_name)
            .hint_text("e.g. My GPT-4o, DeepSeek V3, Local Ollama")
            .desired_width(field_width);
        ui.add(name_edit);
    });
    ui.add_space(8.0);

    // ── Connection ────────────────────────────────────────────────
    card_section(ui, "Connection", |ui| {
        // Provider
        field_row(ui, "Provider", |ui| {
            let current_provider = get_provider_display(&profile.provider);
            egui::ComboBox::from_id_salt("provider_selector")
                .selected_text(&current_provider)
                .width(field_width)
                .show_ui(ui, |ui| {
                    let providers = get_provider_list();
                    for (id, display, default_url) in &providers {
                        let selected = profile.provider == *id;
                        if ui.selectable_label(selected, *display).clicked() {
                            profile.provider = id.to_string();
                            if !default_url.is_empty() {
                                profile.base_url = default_url.to_string();
                            }
                        }
                    }
                });
        });
        ui.add_space(4.0);

        // Base URL
        field_row(ui, "Base URL", |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut profile.base_url)
                    .hint_text("https://api.openai.com/v1")
                    .desired_width(field_width),
            );
        });
        ui.add_space(4.0);

        // API Key
        field_row(ui, "API Key", |ui| {
            let mut key = profile.api_key.clone().unwrap_or_default();
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut key)
                        .hint_text("sk-...")
                        .password(!state.settings.model_profile_show_api_key)
                        .desired_width(api_key_width),
                );
                profile.api_key = if key.is_empty() { None } else { Some(key) };

                let eye = if state.settings.model_profile_show_api_key { "👁" } else { "👁‍🗨" };
                if ui
                    .add(
                        egui::Button::new(eye)
                            .fill(colors::TRANSPARENT)
                            .stroke(Stroke::NONE)
                            .min_size(egui::vec2(28.0, 24.0)),
                    )
                    .on_hover_text("Toggle visibility")
                    .clicked()
                {
                    state.settings.model_profile_show_api_key = !state.settings.model_profile_show_api_key;
                }
            });
        });
        ui.add_space(4.0);

        // Model
        field_row(ui, "Model", |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut profile.model)
                    .hint_text("gpt-4o, claude-3-opus, deepseek-chat, llama3...")
                    .desired_width(field_width),
            );
        });
    });
    ui.add_space(8.0);

    // ── Parameters ────────────────────────────────────────────────
    card_section(ui, "Parameters", |ui| {
        slider_row(ui, "Temperature", &mut profile.temperature, 0.0..=2.0, 0.05, |v| {
            format!("{:.2}", v)
        });

        let mut max_steps = profile.max_steps;
        slider_row(ui, "Max Steps", &mut max_steps, 1..=5000, 10.0, |v| format!("{}", v));
        profile.max_steps = max_steps;

        let mut timeout = profile.timeout_seconds;
        slider_row(ui, "Timeout (s)", &mut timeout, 10..=3600, 30.0, |v| format!("{}s", v));
        profile.timeout_seconds = timeout;

        if let Some(ref mut max_tokens) = profile.max_tokens {
            let mut tokens = *max_tokens;
            slider_row(ui, "Max Tokens", &mut tokens, 1..=128000, 256.0, |v| format!("{}", v));
            *max_tokens = tokens;
        } else {
            ui.horizontal(|ui| {
                ui.colored_label(colors::TEXT_MUTED, RichText::new("Max Tokens:").size(11.0));
                if ui.small_button("Enable").clicked() {
                    profile.max_tokens = Some(4096);
                }
                ui.colored_label(colors::TEXT_MUTED, RichText::new("(optional, uses provider default)").size(11.0));
            });
        }
    });
    ui.add_space(8.0);

    // ── Advanced ──────────────────────────────────────────────────
    card_section(ui, "Advanced", |ui| {
        ui.checkbox(
            &mut profile.thinking_enabled,
            "Enable Thinking (for reasoning models like o1, DeepSeek-R1)",
        );
        if profile.thinking_enabled {
            ui.add_space(4.0);
            if let Some(ref mut budget) = profile.thinking_budget {
                let mut b = *budget;
                slider_row(ui, "Thinking Budget", &mut b, 1..=32768, 256.0, |v| format!("{}", v));
                *budget = b;
            } else {
                ui.horizontal(|ui| {
                    ui.colored_label(colors::TEXT_MUTED, RichText::new("Thinking Budget:").size(11.0));
                    if ui.small_button("Set (8192)").clicked() {
                        profile.thinking_budget = Some(8192);
                    }
                });
            }
        }
    });
    ui.add_space(12.0);

    // ── Action buttons ────────────────────────────────────────────
    ui.separator();
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        // Test Connection
        if ui
            .add(
                egui::Button::new("🔌 Test Connection")
                    .fill(colors::ELEVATED)
                    .stroke(Stroke::new(1.0, colors::BORDER_WEAK))
                    .min_size(egui::vec2(140.0, 28.0)),
            )
            .clicked()
        {
            state.api_commands.push(ApiCommand::TestModelProfileConnection {
                base_url: profile.base_url.clone(),
                api_key: profile.api_key.clone(),
                model: profile.model.clone(),
            });
            state.set_status("Testing connection...".to_string());
        }

        ui.add_space(8.0);

        // Save — always clickable, validates on click
        let name = state.settings.model_profile_edit_name.trim().to_string();

        if ui
            .add(
                egui::Button::new("💾 Save")
                    .fill(colors::RED_ACCENT)
                    .stroke(Stroke::NONE)
                    .min_size(egui::vec2(100.0, 28.0)),
            )
            .on_hover_text("Save profile to backend")
            .clicked()
        {
            if name.is_empty() {
                state.set_status("⚠ Profile name cannot be empty".to_string());
            } else if profile.base_url.is_empty() {
                state.set_status("⚠ Base URL cannot be empty".to_string());
            } else if profile.model.is_empty() {
                state.set_status("⚠ Model name cannot be empty".to_string());
            } else {
                let exists = state.settings.model_profiles.iter().any(|p| p.name == name);

                let mut saved_profile = profile.clone();
                saved_profile.name = name.clone();

                if exists {
                    state.api_commands.push(ApiCommand::UpdateModelProfile {
                        name: name.clone(),
                        profile: saved_profile,
                    });
                } else {
                    state.api_commands.push(ApiCommand::CreateModelProfile {
                        name: name.clone(),
                        profile: saved_profile,
                    });
                }
                state.settings.show_model_profile_create = false;
                state.settings.model_profile_edit_name.clear();
                state.set_status(format!("Saving profile '{}'...", name));
                keep_editing = false;
            }
        }

        ui.add_space(8.0);

        // Cancel
        if ui
            .add(
                egui::Button::new("Cancel")
                    .fill(colors::TRANSPARENT)
                    .stroke(Stroke::NONE),
            )
            .clicked()
        {
            state.settings.show_model_profile_create = false;
            state.settings.model_profile_edit_name.clear();
            keep_editing = false;
        }
    });

    // Validation hints
    if state.settings.model_profile_edit_name.trim().is_empty() {
        ui.add_space(4.0);
        ui.colored_label(colors::WARNING, RichText::new("⚠ Profile name is required").size(11.0));
    } else if profile.model.is_empty() {
        ui.add_space(4.0);
        ui.colored_label(colors::WARNING, RichText::new("⚠ Model name is required").size(11.0));
    } else if profile.base_url.is_empty() {
        ui.add_space(4.0);
        ui.colored_label(colors::WARNING, RichText::new("⚠ Base URL is required").size(11.0));
    }

    keep_editing
}

// ── Helper: card section wrapper ─────────────────────────────────────

fn card_section(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 10))
        .stroke(Stroke::new(1.0, colors::BORDER_WEAK))
        .show(ui, |ui| {
            ui.colored_label(
                colors::TEXT_SECONDARY,
                RichText::new(title).size(12.0).strong(),
            );
            ui.add_space(6.0);
            content(ui);
        });
}

// ── Helper: field row with label ─────────────────────────────────────

fn field_row(ui: &mut egui::Ui, label: &str, content: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.colored_label(colors::TEXT_MUTED, RichText::new(label).size(11.0));
        ui.add_space(2.0);
        content(ui);
    });
}

// ── Helper: slider row with label and value display ──────────────────

fn slider_row<T: egui::emath::Numeric + Copy>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
    step: f64,
    display_fn: impl Fn(T) -> String,
) {
    ui.horizontal(|ui| {
        ui.colored_label(colors::TEXT_MUTED, RichText::new(format!("{}:", label)).size(11.0));
        let slider = egui::Slider::new(value, range).step_by(step).show_value(false);
        ui.add(slider);
        ui.colored_label(colors::TEXT_SECONDARY, RichText::new(display_fn(*value)).size(11.0).strong());
    });
}

// ── Helper: get provider display name ───────────────────────────────

fn get_provider_display(provider: &str) -> String {
    let providers = get_provider_list();
    providers
        .iter()
        .find(|(id, _, _)| id == &provider)
        .map(|(_, display, _)| display.to_string())
        .unwrap_or_else(|| provider.to_string())
}

// ── Helper: get provider list ────────────────────────────────────────

fn get_provider_list() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("openai", "OpenAI", "https://api.openai.com/v1"),
        ("anthropic", "Anthropic", "https://api.anthropic.com"),
        ("deepseek", "DeepSeek", "https://api.deepseek.com/v1"),
        ("gemini", "Google Gemini", "https://generativelanguage.googleapis.com/v1beta"),
        ("ollama", "Ollama (local)", "http://localhost:11434/v1"),
        ("openai-compatible", "OpenAI Compatible (custom)", ""),
        ("mistral", "Mistral", "https://api.mistral.ai/v1"),
        ("openrouter", "OpenRouter", "https://openrouter.ai/api/v1"),
        ("together", "Together AI", "https://api.together.xyz/v1"),
        ("groq", "Groq", "https://api.groq.com/openai/v1"),
    ]
}
