use eframe::egui::{self, CornerRadius};
use uuid::Uuid;

use crate::state::app_state::{AppState, PanelKind};
use crate::theme::colors;

/// Draw the left navigation panel with recent sessions and quick actions
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Frame::NONE
        .fill(colors::GRAPHITE_SURFACE)
        .stroke(egui::Stroke::new(1.0, colors::GRAPHITE_BORDER))
        .inner_margin(egui::Margin::symmetric(12, 12))
        .show(ui, |ui| {
            // ---- Conversations Header ----
            ui.horizontal(|ui| {
                ui.colored_label(colors::RED_ACCENT, "💬");
                ui.add_space(6.0);
                ui.colored_label(
                    egui::Color32::from_rgb(240, 240, 245),
                    egui::RichText::new("Conversations").strong().size(14.0)
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let new_btn = egui::Button::new("+")
                        .fill(colors::RED_DIM)
                        .stroke(egui::Stroke::new(1.0, colors::RED_ACCENT));
                    if ui.add_sized(egui::vec2(28.0, 28.0), new_btn).clicked() {
                        state.chat.create_session(format!("Chat {}", state.chat.sessions.len() + 1));
                        state.set_status("New session created".to_string());
                    }
                });
            });
            ui.add_space(8.0);

            // Search with icon
            egui::Frame::NONE
                .fill(colors::GRAPHITE_ELEVATED)
                .corner_radius(CornerRadius::same(6))
                .stroke(egui::Stroke::new(1.0, colors::GRAPHITE_BORDER))
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(colors::TEXT_MUTED, "🔍");
                        ui.add(
                            egui::TextEdit::singleline(&mut state.chat.search_query)
                                .hint_text("Search...")
                                .frame(false)
                                .desired_width(f32::INFINITY),
                        );
                    });
                });
            ui.add_space(8.0);

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

            egui::ScrollArea::vertical()
                .max_height(ui.available_height() * 0.5)
                .id_source("session_list")
                .show(ui, |ui| {
                    for (id, title, matches_query, msg_count, unread) in &session_data {
                        if !matches_query { continue; }

                        let is_active = Some(*id) == active_id;
                        let bg = if is_active { 
                            colors::RED_DIM 
                        } else { 
                            colors::GRAPHITE_ELEVATED 
                        };
                        
                        let border_color = if is_active {
                            colors::RED_ACCENT
                        } else {
                            colors::GRAPHITE_BORDER
                        };

                        let response = egui::Frame::NONE
                            .fill(bg)
                            .corner_radius(CornerRadius::same(8))
                            .stroke(egui::Stroke::new(1.0, border_color))
                            .inner_margin(egui::Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        if *unread { 
                                            ui.colored_label(colors::RED_ACCENT, "●"); 
                                        } else {
                                            ui.colored_label(colors::TEXT_MUTED, "○");
                                        }
                                        ui.add_space(4.0);
                                        ui.colored_label(
                                            colors::TEXT_PRIMARY,
                                            egui::RichText::new(truncate(title, 26)).size(13.0)
                                        );
                                    });
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        ui.colored_label(colors::TEXT_MUTED, "💬");
                                        ui.add_space(4.0);
                                        ui.colored_label(
                                            colors::TEXT_SECONDARY,
                                            format!("{} messages", msg_count)
                                        );
                                    });
                                });
                            });

                        if response.response.clicked() { to_select = Some(*id); }
                        response.response.context_menu(|ui| {
                            if ui.button("✏️ Rename").clicked() { ui.close_menu(); }
                            if ui.button("🗑️ Delete").clicked() { to_delete = Some(*id); ui.close_menu(); }
                        });
                        ui.add_space(4.0);
                    }

                if is_empty {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| {
                        ui.colored_label(colors::TEXT_MUTED, egui::RichText::new("💬").size(32.0));
                        ui.add_space(8.0);
                        ui.colored_label(colors::TEXT_MUTED, "No conversations yet");
                        ui.add_space(4.0);
                        ui.colored_label(colors::TEXT_SECONDARY, "Click + to start one");
                    });
                }
            });

            if let Some(id) = to_select { state.chat.active_session_id = Some(id); if let Some(s) = state.chat.active_session_mut() { s.unread = false; } }
            if let Some(id) = to_delete { state.chat.delete_session(id); state.set_status("Session deleted".to_string()); }

            // ---- Agent Panels Section ----
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            
            ui.horizontal(|ui| {
                ui.colored_label(colors::RED_ACCENT, "⚙️");
                ui.add_space(6.0);
                ui.colored_label(
                    egui::Color32::from_rgb(240, 240, 245),
                    egui::RichText::new("Agent Controls").strong().size(14.0)
                );
            });
            ui.add_space(8.0);

            nav_button(ui, state, "🎭 Modes", PanelKind::Modes);
            nav_button(ui, state, "🤖 Model Config", PanelKind::ModelConfig);
            nav_button(ui, state, "🧠 Memory", PanelKind::Memory);
            nav_button(ui, state, "📚 Knowledge", PanelKind::Knowledge);
            nav_button(ui, state, "🎙️ Voice", PanelKind::Voice);
            nav_button(ui, state, "🔌 MCP Servers", PanelKind::Mcp);
            nav_button(ui, state, "📊 Audit Log", PanelKind::Audit);
            nav_button(ui, state, "🔍 Diagnostics", PanelKind::Diagnostics);
        });
}

fn nav_button(ui: &mut egui::Ui, state: &mut AppState, label: &str, panel: PanelKind) {
    let is_active = state.show_panel == Some(panel);
    
    let btn = egui::Button::new(label)
        .fill(if is_active { colors::RED_DIM } else { colors::GRAPHITE_ELEVATED })
        .stroke(egui::Stroke::new(1.0, if is_active { colors::RED_ACCENT } else { colors::GRAPHITE_BORDER }));
    
    let response = ui.add_sized(egui::vec2(ui.available_width(), 32.0), btn);
    
    if response.clicked() {
        state.show_panel = if is_active { None } else { Some(panel) };
    }
    
    ui.add_space(2.0);
}

/// Draw only the sessions list (used by dock panel)
pub fn draw_sessions_list(ui: &mut egui::Ui, state: &mut AppState) {
    // Search with icon
    egui::Frame::NONE
        .fill(colors::GRAPHITE_ELEVATED)
        .corner_radius(CornerRadius::same(6))
        .stroke(egui::Stroke::new(1.0, colors::GRAPHITE_BORDER))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(colors::TEXT_MUTED, "🔍");
                ui.add(
                    egui::TextEdit::singleline(&mut state.chat.search_query)
                        .hint_text("Search...")
                        .frame(false)
                        .desired_width(f32::INFINITY),
                );
            });
        });
    ui.add_space(8.0);

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

    for (id, title, matches_query, msg_count, unread) in &session_data {
        if !matches_query { continue; }

        let is_active = Some(*id) == active_id;
        let bg = if is_active { 
            colors::RED_DIM 
        } else { 
            colors::GRAPHITE_ELEVATED 
        };
        
        let border_color = if is_active {
            colors::RED_ACCENT
        } else {
            colors::GRAPHITE_BORDER
        };

        let response = egui::Frame::NONE
            .fill(bg)
            .corner_radius(CornerRadius::same(8))
            .stroke(egui::Stroke::new(1.0, border_color))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if *unread { 
                            ui.colored_label(colors::RED_ACCENT, "●"); 
                        } else {
                            ui.colored_label(colors::TEXT_MUTED, "○");
                        }
                        ui.add_space(4.0);
                        ui.colored_label(
                            colors::TEXT_PRIMARY,
                            egui::RichText::new(truncate(title, 26)).size(13.0)
                        );
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.colored_label(colors::TEXT_MUTED, "💬");
                        ui.add_space(4.0);
                        ui.colored_label(
                            colors::TEXT_SECONDARY,
                            format!("{} messages", msg_count)
                        );
                    });
                });
            });

        if response.response.clicked() { to_select = Some(*id); }
        response.response.context_menu(|ui| {
            if ui.button("✏️ Rename").clicked() { ui.close_menu(); }
            if ui.button("🗑️ Delete").clicked() { to_delete = Some(*id); ui.close_menu(); }
        });
        ui.add_space(4.0);
    }

    if is_empty {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.colored_label(colors::TEXT_MUTED, egui::RichText::new("💬").size(32.0));
            ui.add_space(8.0);
            ui.colored_label(colors::TEXT_MUTED, "No conversations yet");
            ui.add_space(4.0);
            ui.colored_label(colors::TEXT_SECONDARY, "Click + to start one");
        });
    }

    if let Some(id) = to_select { state.chat.active_session_id = Some(id); if let Some(s) = state.chat.active_session_mut() { s.unread = false; } }
    if let Some(id) = to_delete { state.chat.delete_session(id); state.set_status("Session deleted".to_string()); }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}…", truncated)
    }
}
