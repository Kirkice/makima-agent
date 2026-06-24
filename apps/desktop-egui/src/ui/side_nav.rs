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

fn draw_sessions(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(
                    colors::TEXT_PRIMARY,
                    egui::RichText::new("Recent").size(13.0).strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let button = egui::Button::new("+ New")
                        .fill(colors::RED_ACCENT)
                        .stroke(egui::Stroke::NONE)
                        .min_size(egui::vec2(64.0, 26.0));
                    if ui.add(button).clicked() {
                        state
                            .chat
                            .create_session(format!("Chat {}", state.chat.sessions.len() + 1));
                        state.set_status("New session created".to_string());
                    }
                });
            });
        });
    ui.add_space(8.0);

    let active_id = state.chat.active_session_id;
    let is_empty = state.chat.sessions.is_empty();
    let mut to_select = None;

    if !is_empty {
        draw_search(ui, &mut state.chat.search_query, "Search conversations");
        ui.add_space(10.0);
    }

    // Use remaining vertical space for the session list
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
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            if session.unread {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(8.0, 8.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter()
                                    .circle_filled(rect.center(), 4.0, colors::RED_ACCENT);
                            } else {
                                ui.add_sized(egui::vec2(8.0, 8.0), egui::Label::new(""));
                            }
                            ui.add_space(4.0);
                            let text_width = (ui.available_width() - 44.0).max(60.0);
                            ui.vertical(|ui| {
                                ui.set_width(text_width);
                                ui.colored_label(
                                    colors::TEXT_PRIMARY,
                                    egui::RichText::new(truncate_to_width(&session.title, text_width)).size(13.0),
                                );
                                ui.colored_label(
                                    colors::TEXT_MUTED,
                                    format!("{} msg{}", session.messages.len(), if session.messages.len() == 1 { "" } else { "s" }),
                                );
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                if ui.small_button("🗑").on_hover_text("Delete conversation").clicked() {
                                    to_delete = Some(session.id);
                                }
                            });
                        });
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

            if is_empty {
                empty_state(ui, "No conversations yet", "Start a new chat to build your workspace.");
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
    kv_card(
        ui,
        "Memory",
        &format!("{} items cached", state.settings.memory_items.len()),
        colors::INFO,
        Some(|| state.api_commands.push(ApiCommand::FetchMemories)),
    );
    kv_card(
        ui,
        "Knowledge",
        &format!("{} docs indexed", state.settings.knowledge_docs.len()),
        colors::SUCCESS,
        Some(|| state.api_commands.push(ApiCommand::FetchDocuments)),
    );
}

fn draw_agent(ui: &mut egui::Ui, state: &mut AppState) {
    let mode_name = state
        .settings
        .active_mode()
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "No mode selected".to_string());
    let persona_name = if state.settings.persona_name.is_empty() {
        "Default persona".to_string()
    } else {
        state.settings.persona_name.clone()
    };
    let model_name = if state.settings.model_config.configured {
        state.settings.model_config.model.clone()
    } else {
        "Model not configured".to_string()
    };

    kv_card_static(ui, "Mode", &mode_name, colors::RED_ACCENT);
    kv_card_static(ui, "Persona", &persona_name, colors::WARNING);
    kv_card_static(ui, "Model", &model_name, colors::INFO);
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

    kv_card_static(ui, "Voice", voice_label, voice_color);
    kv_card_static(
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
    );

    ui.add_space(8.0);
    if ui.button("Open diagnostics").clicked() {
        state.drawer_open = true;
        state.drawer_tab = Some(crate::state::app_state::DrawerTab::Diagnostics);
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
        ui.colored_label(colors::TEXT_MUTED, subtitle);

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
        });
}

fn kv_card<F: FnOnce()>(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    accent: egui::Color32,
    refresh: Option<F>,
) {
    let mut cb = refresh;
    egui::Frame::NONE
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

            ui.horizontal(|ui| {
                ui.colored_label(colors::TEXT_SECONDARY, label);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(accent, egui::RichText::new(value).size(13.0));
                    if let Some(f) = cb.take() {
                        if ui.small_button("Refresh").on_hover_text("Refresh").clicked() {
                            f();
                        }
                    }
                });
            });
        });
    ui.add_space(6.0);
}

fn kv_card_static(ui: &mut egui::Ui, label: &str, value: &str, accent: egui::Color32) {
    kv_card::<fn()>(ui, label, value, accent, None);
}

fn empty_state(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.add_space(32.0);
    ui.centered_and_justified(|ui| {
        ui.vertical(|ui| {
            ui.colored_label(colors::TEXT_PRIMARY, egui::RichText::new(title).strong());
            ui.colored_label(colors::TEXT_MUTED, subtitle);
        });
    });
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
