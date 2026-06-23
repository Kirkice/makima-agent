use eframe::egui::{self, CornerRadius};
use uuid::Uuid;

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
    ui.horizontal(|ui| {
        ui.colored_label(colors::TEXT_PRIMARY, egui::RichText::new("Recent").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let button = egui::Button::new("+ New")
                .fill(colors::ELEVATED)
                .stroke(egui::Stroke::NONE);
            if ui.add(button).clicked() {
                state
                    .chat
                    .create_session(format!("Chat {}", state.chat.sessions.len() + 1));
                state.set_status("New session created".to_string());
            }
        });
    });
    ui.add_space(8.0);

    draw_search(ui, &mut state.chat.search_query, "Search conversations");
    ui.add_space(12.0);

    let session_data: Vec<(Uuid, String, bool, usize, bool)> = state
        .chat
        .sessions
        .iter()
        .map(|s| {
            let matches = state.chat.search_query.is_empty()
                || s.title
                    .to_lowercase()
                    .contains(&state.chat.search_query.to_lowercase());
            (s.id, s.title.clone(), matches, s.messages.len(), s.unread)
        })
        .collect();

    let active_id = state.chat.active_session_id;
    let is_empty = state.chat.sessions.is_empty();
    let mut to_select = None;

    egui::ScrollArea::vertical()
        .id_salt("conversation_sidebar_list")
        .show(ui, |ui| {
            for (id, title, matches, msg_count, unread) in &session_data {
                if !matches {
                    continue;
                }

                let selected = Some(*id) == active_id;
                let response = egui::Frame::NONE
                    .fill(if selected {
                        colors::SELECTION_SOFT
                    } else {
                        colors::TRANSPARENT
                    })
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                if *unread {
                                    colors::RED_ACCENT
                                } else {
                                    colors::TEXT_MUTED
                                },
                                "●",
                            );
                            ui.add_space(6.0);
                            ui.vertical(|ui| {
                                ui.colored_label(
                                    colors::TEXT_PRIMARY,
                                    egui::RichText::new(truncate(title, 28)).size(13.0),
                                );
                                ui.colored_label(
                                    colors::TEXT_MUTED,
                                    format!("{msg_count} messages"),
                                );
                            });
                        });
                    });

                if response.response.clicked() {
                    to_select = Some(*id);
                }

                ui.add_space(4.0);
            }

            if is_empty {
                empty_state(
                    ui,
                    "No conversations yet",
                    "Start a new chat to build your workspace.",
                );
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
    summary_card(
        ui,
        "Memory",
        format!("{} items cached", state.settings.memory_items.len()).as_str(),
        "Refresh",
        || state.api_commands.push(ApiCommand::FetchMemories),
    );
    summary_card(
        ui,
        "Knowledge",
        format!("{} docs indexed", state.settings.knowledge_docs.len()).as_str(),
        "Load",
        || state.api_commands.push(ApiCommand::FetchDocuments),
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

    info_card(ui, "Mode", &mode_name);
    info_card(ui, "Persona", &persona_name);
    info_card(ui, "Model", &model_name);
}

fn draw_integrations(ui: &mut egui::Ui, state: &mut AppState) {
    let voice_status = if state.voice_call.is_connected {
        "Voice connected"
    } else if state.voice_call.is_connecting {
        "Voice connecting"
    } else {
        "Voice idle"
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

    info_card(ui, "Voice", voice_status);
    info_card(
        ui,
        "MCP",
        &format!("{mcp_connected}/{} connected", state.settings.mcp_servers.len()),
    );

    ui.add_space(4.0);
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
            egui::RichText::new(title).size(15.0).strong(),
        );
        ui.colored_label(colors::TEXT_MUTED, subtitle);
    });
}

fn draw_search(ui: &mut egui::Ui, value: &mut String, hint: &str) {
    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(colors::TEXT_MUTED, "⌕");
                ui.add(
                    egui::TextEdit::singleline(value)
                        .hint_text(hint)
                        .frame(false)
                        .desired_width(f32::INFINITY),
                );
            });
        });
}

fn summary_card<F: FnOnce()>(ui: &mut egui::Ui, title: &str, body: &str, action: &str, on_click: F) {
    let mut callback = Some(on_click);
    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.colored_label(
                colors::TEXT_PRIMARY,
                egui::RichText::new(title).size(14.0).strong(),
            );
            ui.colored_label(colors::TEXT_MUTED, body);
            ui.add_space(8.0);
            if ui.button(action).clicked() {
                if let Some(cb) = callback.take() {
                    cb();
                }
            }
        });
    ui.add_space(8.0);
}

fn info_card(ui: &mut egui::Ui, title: &str, body: &str) {
    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.colored_label(colors::TEXT_MUTED, title);
            ui.colored_label(
                colors::TEXT_PRIMARY,
                egui::RichText::new(body).size(13.0).strong(),
            );
        });
    ui.add_space(8.0);
}

fn empty_state(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        ui.colored_label(colors::TEXT_MUTED, egui::RichText::new("○").size(28.0));
        ui.colored_label(colors::TEXT_PRIMARY, egui::RichText::new(title).strong());
        ui.colored_label(colors::TEXT_MUTED, subtitle);
    });
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}
