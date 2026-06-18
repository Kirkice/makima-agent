use eframe::egui::{self, Rounding};
use uuid::Uuid;

use crate::state::app_state::{AppState, PanelKind};
use crate::theme::colors;

/// Draw the left navigation panel with recent sessions and quick actions
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Frame::none()
        .fill(colors::GRAPHITE_SURFACE)
        .inner_margin(egui::Margin::symmetric(8.0, 8.0))
        .show(ui, |ui| {
            // ---- Conversations Header ----
            ui.horizontal(|ui| {
                ui.colored_label(colors::TEXT_SECONDARY, "Conversations");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("+").clicked() {
                        state.chat.create_session(format!("Chat {}", state.chat.sessions.len() + 1));
                        state.set_status("New session created".to_string());
                    }
                });
            });
            ui.separator();
            ui.add_space(4.0);

            // Search
            ui.add(
                egui::TextEdit::singleline(&mut state.chat.search_query)
                    .hint_text("Search conversations...")
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(4.0);

            // Session list
            let session_data: Vec<(Uuid, String, bool, usize, bool)> = state
                .chat.sessions.iter()
                .map(|s| {
                    let matches = state.chat.search_query.is_empty()
                        || s.title.to_lowercase().contains(&state.chat.search_query.to_lowercase());
                    (s.id, s.title.clone(), matches, s.messages.len(), s.unread)
                })
                .collect();
            let active_id = state.chat.active_session_id;
            let is_empty = state.chat.sessions.is_empty();

            let mut to_delete: Option<Uuid> = None;
            let mut to_select: Option<Uuid> = None;

            egui::ScrollArea::vertical().id_source("session_list").show(ui, |ui| {
                for (id, title, matches_query, msg_count, unread) in &session_data {
                    if !matches_query { continue; }

                    let is_active = Some(*id) == active_id;
                    let bg = if is_active { colors::RED_DIM } else { colors::GRAPHITE_ELEVATED };

                    let response = egui::Frame::none().fill(bg).rounding(Rounding::same(6.0))
                        .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    if *unread { ui.colored_label(colors::RED_ACCENT, "●"); }
                                    ui.label(truncate(title, 28));
                                });
                                ui.colored_label(colors::TEXT_MUTED, format!("{} msgs", msg_count));
                            });
                        });

                    if response.response.clicked() { to_select = Some(*id); }
                    response.response.context_menu(|ui| {
                        if ui.button("Rename").clicked() { ui.close_menu(); }
                        if ui.button("Delete").clicked() { to_delete = Some(*id); ui.close_menu(); }
                    });
                    ui.add_space(2.0);
                }

                if is_empty {
                    ui.add_space(16.0);
                    ui.colored_label(colors::TEXT_MUTED, "No conversations yet.\nClick + to start one.");
                }
            });

            if let Some(id) = to_select { state.chat.active_session_id = Some(id); if let Some(s) = state.chat.active_session_mut() { s.unread = false; } }
            if let Some(id) = to_delete { state.chat.delete_session(id); state.set_status("Session deleted".to_string()); }

            // ---- Agent Panels Section ----
            ui.add_space(8.0);
            ui.separator();
            ui.colored_label(colors::RED_ACCENT, "Agent Controls");
            ui.add_space(4.0);

            nav_button(ui, state, "Modes", PanelKind::Modes);
            nav_button(ui, state, "Model Config", PanelKind::ModelConfig);
            nav_button(ui, state, "Memory", PanelKind::Memory);
            nav_button(ui, state, "Knowledge", PanelKind::Knowledge);
            nav_button(ui, state, "Voice", PanelKind::Voice);
            nav_button(ui, state, "MCP Servers", PanelKind::Mcp);
            nav_button(ui, state, "Audit Log", PanelKind::Audit);
            nav_button(ui, state, "Diagnostics", PanelKind::Diagnostics);
        });
}

fn nav_button(ui: &mut egui::Ui, state: &mut AppState, label: &str, panel: PanelKind) {
    let is_active = state.show_panel == Some(panel);
    if ui.add_sized(egui::vec2(ui.available_width(), 24.0), egui::Button::new(label).fill(if is_active { colors::RED_DIM } else { colors::GRAPHITE_ELEVATED })).clicked() {
        state.show_panel = if is_active { None } else { Some(panel) };
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars { s.to_string() } else { format!("{}…", &s[..max_chars]) }
}