use eframe::egui::{self, CornerRadius};

use crate::state::app_state::{ActivitySection, ApiCommand, AppState};
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.vertical(|ui| {
        section_header(ui, state.activity_section);
        ui.add_space(12.0);

        match state.activity_section {
            ActivitySection::Sessions => draw_sessions(ui, state),
            ActivitySection::Resources => draw_resources(ui, state),
            ActivitySection::Agent => draw_agent(ui, state),
            ActivitySection::Integrations => draw_integrations(ui, state),
        }
    });
}

fn is_narrow(ui: &egui::Ui) -> bool {
    ui.available_width() < 220.0
}

fn draw_sessions(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            let new_button = egui::Button::new("+ New")
                .fill(colors::RED_ACCENT)
                .stroke(egui::Stroke::NONE)
                .min_size(egui::vec2(64.0, 26.0));

            if is_narrow(ui) {
                ui.vertical(|ui| {
                    ui.colored_label(
                        colors::TEXT_PRIMARY,
                        egui::RichText::new("Recent").size(13.0).strong(),
                    );
                    if ui.add_sized([ui.available_width(), 26.0], new_button).clicked() {
                        state
                            .chat
                            .create_session(format!("Chat {}", state.chat.sessions.len() + 1));
                        state.set_status("New session created".to_string());
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        colors::TEXT_PRIMARY,
                        egui::RichText::new("Recent").size(13.0).strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(new_button).clicked() {
                            state
                                .chat
                                .create_session(format!("Chat {}", state.chat.sessions.len() + 1));
                            state.set_status("New session created".to_string());
                        }
                    });
                });
            }
        });
    ui.add_space(8.0);

    let active_id = state.chat.active_session_id;
    let is_empty = state.chat.sessions.is_empty();
    let mut to_select = None;

    if !is_empty {
        draw_search(ui, &mut state.chat.search_query, "Search conversations");
        ui.add_space(10.0);
    }

    let available = ui.available_size_before_wrap();
    let scroll_h = (available.y - 4.0).max(0.0);
    egui::ScrollArea::vertical()
        .id_salt("conversation_sidebar_list")
        .auto_shrink([false, false])
        .max_height(scroll_h)
        .show(ui, |ui| {
            let mut to_delete = None;

            for session in &state.chat.sessions {
                let matches = state.chat.search_query.is_empty()
                    || session
                        .title
                        .to_lowercase()
                        .contains(&state.chat.search_query.to_lowercase());
                if !matches {
                    continue;
                }

                let selected = Some(session.id) == active_id;
                let response = egui::Frame::NONE
                    .fill(if selected {
                        colors::SELECTION_SOFT
                    } else {
                        colors::TRANSPARENT
                    })
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        if ui.available_width() < 170.0 {
                            ui.vertical(|ui| {
                                let label_w = ui.available_width().max(60.0);
                                ui.colored_label(
                                    colors::TEXT_PRIMARY,
                                    egui::RichText::new(truncate_to_width(&session.title, label_w)).size(13.0),
                                );
                                ui.colored_label(
                                    colors::TEXT_MUTED,
                                    egui::RichText::new(format!("{} msg", session.messages.len())).size(11.0),
                                );
                                let del_btn = egui::Button::new("🗑")
                                    .fill(colors::TRANSPARENT)
                                    .stroke(egui::Stroke::NONE)
                                    .min_size(egui::vec2(22.0, 22.0));
                                if ui.add(del_btn).on_hover_text("Delete conversation").clicked() {
                                    to_delete = Some(session.id);
                                }
                            });
                        } else {
                            ui.horizontal(|ui| {
                                if session.unread {
                                    let (dot_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(6.0, 6.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter()
                                        .circle_filled(dot_rect.center(), 3.0, colors::RED_ACCENT);
                                    ui.add_space(6.0);
                                } else {
                                    ui.add_sized(egui::vec2(6.0, 6.0), egui::Label::new(""));
                                    ui.add_space(6.0);
                                }

                                let label_w = (ui.available_width() - 30.0).max(40.0);
                                ui.vertical(|ui| {
                                    ui.set_width(label_w);
                                    ui.colored_label(
                                        colors::TEXT_PRIMARY,
                                        egui::RichText::new(truncate_to_width(&session.title, label_w)).size(13.0),
                                    );
                                    ui.colored_label(
                                        colors::TEXT_MUTED,
                                        egui::RichText::new(format!("{} msg", session.messages.len())).size(11.0),
                                    );
                                });

                                let del_btn = egui::Button::new("🗑")
                                    .fill(colors::TRANSPARENT)
                                    .stroke(egui::Stroke::NONE)
                                    .min_size(egui::vec2(22.0, 22.0));
                                if ui.add(del_btn).on_hover_text("Delete conversation").clicked() {
                                    to_delete = Some(session.id);
                                }
                            });
                        }
                    });

                if response.response.clicked() {
                    to_select = Some(session.id);
                }
                ui.add_space(4.0);
            }

            if let Some(delete_id) = to_delete {
                let id_str = delete_id.to_string();
                state.api_commands.push(ApiCommand::DeleteSession(id_str));
                state.chat.delete_session(delete_id);
                state.set_status("Conversation deleted".to_string());
            }
        });

    if let Some(id) = to_select {
        state.chat.active_session_id = Some(id);
        if let Some(session) = state.chat.active_session_mut() {
            session.unread = false;
        }
    }
}

fn draw_resources(ui: &mut egui::Ui, state: &mut AppState) {
    if kv_card_interactive(ui, "Memory", &format!("{} items cached", state.settings.memory_items.len()), colors::INFO) {
        state.show_window_memory = true;
        state.api_commands.push(ApiCommand::FetchMemories);
    }
    if kv_card_interactive(ui, "Knowledge", &format!("{} docs indexed", state.settings.knowledge_docs.len()), colors::SUCCESS) {
        state.show_window_knowledge = true;
        state.api_commands.push(ApiCommand::FetchDocuments);
    }
}

fn draw_agent(ui: &mut egui::Ui, state: &mut AppState) {
    let mode_name = state
        .settings
        .active_mode()
        .map(|m| {
            let name = &m.name;
            if let Some(idx) = name.find(|c: char| c.is_alphabetic()) {
                if idx > 0 {
                    format!("{}{}", name[..idx].trim_end(), &name[idx..])
                } else {
                    name.clone()
                }
            } else {
                name.clone()
            }
        })
        .unwrap_or_else(|| "No mode selected".to_string());
    let persona_name = if state.settings.persona_name.is_empty() {
        "Default persona".to_string()
    } else {
        state.settings.persona_name.clone()
    };
    let model_name = state
        .settings
        .active_model_profile
        .clone()
        .unwrap_or_else(|| {
            if state.settings.model_config.configured {
                format!("{} ({})", state.settings.model_config.model, state.settings.model_config.provider)
            } else {
                "Not configured".to_string()
            }
        });

    if kv_card_interactive(ui, "Mode", &mode_name, colors::RED_ACCENT) {
        state.show_window_modes = true;
        state.api_commands.push(ApiCommand::FetchModes);
    }
    if kv_card_interactive(ui, "Persona", &persona_name, colors::WARNING) {
        state.show_window_persona = true;
        state.api_commands.push(ApiCommand::FetchPersona);
    }
    if kv_card_interactive(ui, "Model", &model_name, colors::INFO) {
        state.show_window_model_config = true;
        state.api_commands.push(ApiCommand::FetchModelProfiles);
    }
}

fn draw_integrations(ui: &mut egui::Ui, state: &mut AppState) {
    let (voice_label, voice_color) = if state.voice_call.is_connected {
        ("Connected", colors::SUCCESS)
    } else if state.voice_call.is_connecting {
        ("Connecting", colors::WARNING)
    } else {
        ("Idle", colors::TEXT_MUTED)
    };
    let mcp_connected = state
        .settings
        .mcp_servers
        .iter()
        .filter(|srv| {
            matches!(
                srv.status,
                crate::state::settings_state::McpConnectionStatus::Connected
            )
        })
        .count();
    let mcp_total = state.settings.mcp_servers.len();

    if kv_card_interactive(ui, "Voice", voice_label, voice_color) {
        state.show_window_voice = true;
        state.api_commands.push(ApiCommand::FetchVoiceSettings);
    }
    if kv_card_interactive(
        ui,
        "MCP",
        &format!("{} / {} connected", mcp_connected, mcp_total),
        if mcp_total > 0 && mcp_connected == mcp_total {
            colors::SUCCESS
        } else if mcp_connected > 0 {
            colors::WARNING
        } else {
            colors::TEXT_MUTED
        },
    ) {
        state.show_window_mcp = true;
        state.api_commands.push(ApiCommand::FetchMcpServers);
    }

    ui.add_space(8.0);
    if ui
        .add_sized([ui.available_width(), 28.0], egui::Button::new("Open diagnostics"))
        .clicked()
    {
        state.show_window_diagnostics = true;
        state.api_commands.push(ApiCommand::RefreshHealth);
    }
}

fn section_header(ui: &mut egui::Ui, section: ActivitySection) {
    let (title, subtitle) = match section {
        ActivitySection::Sessions => ("Conversations", "Recent chats and active threads"),
        ActivitySection::Resources => ("Resources", "Memory and knowledge sources"),
        ActivitySection::Agent => ("Agent", "Current behavior and identity"),
        ActivitySection::Integrations => ("Integrations", "Voice, MCP and runtime health"),
    };

    ui.vertical(|ui| {
        ui.colored_label(
            colors::TEXT_PRIMARY,
            egui::RichText::new(title).size(16.0).strong(),
        );
        ui.add(egui::Label::new(egui::RichText::new(subtitle).color(colors::TEXT_MUTED)).wrap());

        let sep_rect = egui::Rect::from_min_size(
            ui.cursor().min + egui::vec2(0.0, 4.0),
            egui::vec2(ui.available_width(), 1.0),
        );
        ui.painter()
            .rect_filled(sep_rect, CornerRadius::ZERO, colors::BORDER_WEAK);
    });
}

fn draw_search(ui: &mut egui::Ui, value: &mut String, hint: &str) {
    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            if is_narrow(ui) {
                ui.vertical(|ui| {
                    ui.colored_label(colors::TEXT_MUTED, "Search");
                    ui.add(
                        egui::TextEdit::singleline(value)
                            .hint_text(hint)
                            .frame(false)
                            .desired_width(f32::INFINITY),
                    );
                });
            } else {
                ui.horizontal(|ui| {
                    ui.colored_label(colors::TEXT_MUTED, "Search");
                    ui.add_space(8.0);
                    ui.add(
                        egui::TextEdit::singleline(value)
                            .hint_text(hint)
                            .frame(false)
                            .desired_width(f32::INFINITY),
                    );
                });
            }
        });
}

// kv_card and kv_card_static removed — all cards now use kv_card_interactive.

fn kv_card_interactive(ui: &mut egui::Ui, label: &str, value: &str, accent: egui::Color32) -> bool {
    let response = egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin {
            left: 12,
            right: 10,
            top: 9,
            bottom: 9,
        })
        .show(ui, |ui| {
            let bar_rect = egui::Rect::from_min_size(
                ui.min_rect().min,
                egui::vec2(3.0, ui.min_rect().height()),
            );
            ui.painter()
                .rect_filled(bar_rect, CornerRadius::same(2), accent);

            if is_narrow(ui) {
                ui.vertical(|ui| {
                    ui.colored_label(colors::TEXT_SECONDARY, label);
                    ui.add(egui::Label::new(egui::RichText::new(value).size(13.0).color(accent)).wrap());
                });
            } else {
                ui.horizontal(|ui| {
                    ui.colored_label(colors::TEXT_SECONDARY, label);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.colored_label(accent, egui::RichText::new(value).size(13.0));
                    });
                });
            }
        });
    ui.add_space(6.0);
    response.response.clicked()
}

fn truncate_to_width(s: &str, max_width: f32) -> String {
    let avg_char_w = 8.0;
    let max_chars = (max_width / avg_char_w).max(1.0) as usize;
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    }
}
