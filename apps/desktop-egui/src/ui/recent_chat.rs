 use chrono::{Local, TimeZone, Utc};
use eframe::egui::{self, CornerRadius, Sense, Vec2};

use crate::state::app_state::{ApiCommand, AppState};
use crate::state::chat_state::Session;
use crate::theme::colors;

/// Codex-style product sidebar — clean, modern, grouped conversations.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.vertical(|ui| {
        brand_header(ui);
        ui.add_space(12.0);

        new_chat_button(ui, state);
        ui.add_space(10.0);

        search_box(ui, state);
        ui.add_space(8.0);

        sessions_list(ui, state);

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            footer(ui, state);
        });
    });
}

// ── Brand ────────────────────────────────────────────────────────

fn brand_header(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
        let center = dot_rect.center();
        ui.painter()
            .circle_filled(egui::pos2(center.x + 4.0, center.y + 4.0), 4.5, colors::RED_ACCENT);
        ui.add_space(2.0);
        ui.colored_label(
            colors::TEXT_PRIMARY,
            egui::RichText::new("Makima").size(16.0).strong(),
        );
    });
    ui.add_space(2.0);
    ui.colored_label(
        colors::TEXT_MUTED,
        egui::RichText::new("Agent Workspace").size(11.0),
    );
}

// ── New Chat Button ───────────────────────────────────────────────

fn new_chat_button(ui: &mut egui::Ui, state: &mut AppState) {
    let btn = egui::Button::new(
        egui::RichText::new("＋ New Chat").size(13.0).color(colors::TEXT_PRIMARY),
    )
    .fill(colors::TRANSPARENT)
    .stroke(egui::Stroke::new(1.0, colors::RED_ACCENT))
    .corner_radius(CornerRadius::same(8))
    .min_size(egui::vec2(0.0, 36.0));

    if ui
        .add_sized([ui.available_width(), 36.0], btn)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        state
            .chat
            .create_session(format!("Chat {}", state.chat.sessions.len() + 1));
        state.set_status("New session created".to_string());
    }
}

// ── Search ────────────────────────────────────────────────────────

fn search_box(ui: &mut egui::Ui, state: &mut AppState) {
    if state.chat.sessions.is_empty() {
        return;
    }

    let (rect, mut response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 32.0),
        Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        let is_focused = response.has_focus();
        let stroke_color = if is_focused {
            colors::RED_ACCENT
        } else if response.hovered() {
            colors::TEXT_MUTED
        } else {
            colors::GRAPHITE_BORDER
        };

        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), colors::ELEVATED);
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(8),
            egui::Stroke::new(1.0, stroke_color),
            egui::StrokeKind::Inside,
        );

        let inner = rect.shrink2(egui::vec2(10.0, 6.0));
        let mut text_ui = ui.child_ui(
            inner,
            egui::Layout::left_to_right(egui::Align::Center),
            None,
        );
        text_ui.set_clip_rect(inner);

        if state.chat.search_query.is_empty() && !is_focused {
            text_ui.colored_label(colors::TEXT_MUTED, "🔍  Search chats…");
        } else {
            let text_response = text_ui.add(
                egui::TextEdit::singleline(&mut state.chat.search_query)
                    .hint_text("Search chats…")
                    .frame(false)
                    .desired_width(inner.width() - 24.0),
            );
            if text_response.clicked() {
                response.request_focus();
            }
        }
    }

    if response.clicked() {
        response.request_focus();
    }
}

// ── Session List (grouped by date) ────────────────────────────────

struct SessionCard {
    id: uuid::Uuid,
    title: String,
    last_msg_preview: String,
    num_msgs: usize,
    updated_at: chrono::DateTime<Utc>,
    unread: bool,
}

fn sessions_list(ui: &mut egui::Ui, state: &mut AppState) {
    let active_id = state.chat.active_session_id;
    let query = state.chat.search_query.to_lowercase();

    let mut cards: Vec<SessionCard> = state
        .chat
        .sessions
        .iter()
        .filter(|s| {
            query.is_empty()
                || s.title.to_lowercase().contains(&query)
                || s.messages.iter().any(|m| {
                    m.text
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&query)
                })
        })
        .map(|s| SessionCard {
            id: s.id,
            title: s.title.clone(),
            last_msg_preview: s
                .messages
                .last()
                .and_then(|m| m.text.as_deref())
                .unwrap_or("")
                .chars()
                .take(60)
                .collect(),
            num_msgs: s.messages.len(),
            updated_at: s.updated_at,
            unread: s.unread,
        })
        .collect();

    cards.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    if cards.is_empty() && !state.chat.sessions.is_empty() {
        ui.add_space(16.0);
        ui.centered_and_justified(|ui| {
            ui.colored_label(colors::TEXT_MUTED, "No matching chats");
        });
        return;
    }

    if cards.is_empty() {
        return;
    }

    let now = Local::now();
    let today = now.date_naive();
    let yesterday = today.pred_opt().unwrap_or(today);

    let mut groups: Vec<(&str, Vec<&SessionCard>)> = Vec::new();

    for card in &cards {
        let date = card.updated_at.with_timezone(&Local).date_naive();
        let label = if date == today {
            "Today"
        } else if date == yesterday {
            "Yesterday"
        } else {
            "Older"
        };
        if let Some((_, group)) = groups.iter_mut().find(|(l, _)| *l == label) {
            group.push(card);
        } else {
            groups.push((label, vec![card]));
        }
    }

    let order = ["Today", "Yesterday", "Older"];
    groups.sort_by_key(|(label, _)| order.iter().position(|o| o == label).unwrap_or(99));

    let available = ui.available_size_before_wrap();
    let scroll_h = (available.y - 4.0).max(0.0);

    let mut to_select: Option<uuid::Uuid> = None;
    let mut to_delete: Option<uuid::Uuid> = None;

    egui::ScrollArea::vertical()
        .id_salt("sidebar_sessions")
        .auto_shrink([false, false])
        .max_height(scroll_h)
        .show(ui, |ui| {
            for (group_label, group_cards) in &groups {
                ui.add_space(4.0);
                ui.colored_label(
                    colors::TEXT_MUTED,
                    egui::RichText::new(*group_label).size(11.0).strong(),
                );
                ui.add_space(4.0);

                for card in group_cards {
                    let selected = Some(card.id) == active_id;

                    let mut delete_flag: Option<uuid::Uuid> = None;
                    let response = egui::Frame::NONE
                        .fill(if selected {
                            colors::ELEVATED
                        } else {
                            colors::TRANSPARENT
                        })
                        .stroke(if selected {
                            egui::Stroke::new(1.0, colors::RED_ACCENT)
                        } else {
                            egui::Stroke::NONE
                        })
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            draw_session_card(ui, card, selected, &mut delete_flag);
                        });

                    let clicked = response.response.hovered()
                        && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));
                    if clicked {
                        to_select = Some(card.id);
                    }
                    if let Some(del_id) = delete_flag {
                        to_delete = Some(del_id);
                    }

                    ui.add_space(2.0);
                }
            }
        });

    if let Some(delete_id) = to_delete {
        let id_str = delete_id.to_string();
        state
            .api_commands
            .push(ApiCommand::DeleteSession(id_str));
        state.chat.delete_session(delete_id);
        state.set_status("Conversation deleted".to_string());
    }

    if let Some(id) = to_select {
        state.chat.active_session_id = Some(id);
        if let Some(session) = state.chat.active_session_mut() {
            session.unread = false;
        }
    }
}

fn draw_session_card(
    ui: &mut egui::Ui,
    card: &SessionCard,
    selected: bool,
    to_delete: &mut Option<uuid::Uuid>,
) {
    let relative_time = relative_time_str(card.updated_at);

    ui.horizontal(|ui| {
        // Accent bar for selected
        if selected {
            let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(3.0, 14.0), Sense::hover());
            ui.painter()
                .rect_filled(bar_rect, CornerRadius::same(2), colors::RED_ACCENT);
            ui.add_space(4.0);
        }

        // Unread dot
        if card.unread && !selected {
            let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 8.0), Sense::hover());
            ui.painter()
                .circle_filled(dot_rect.center(), 3.0, colors::RED_ACCENT);
            ui.add_space(4.0);
        }

        // Title
        let right_width = 58.0; // delete btn + time
        let title_w = (ui.available_width() - right_width - 4.0).max(40.0);
        let title_text = truncate_label(ui, &card.title, title_w);
        ui.colored_label(
            colors::TEXT_PRIMARY,
            egui::RichText::new(title_text).size(13.0),
        );

        // Right side: time + delete
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.colored_label(
                colors::TEXT_MUTED,
                egui::RichText::new(relative_time).size(11.0),
            );
            ui.add_space(2.0);
            // Manual delete icon — avoids button padding misalignment
            let (del_rect, del_resp) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::click());
            let galley = ui.painter().layout_no_wrap(
                "🗑".to_string(),
                egui::FontId::proportional(12.0),
                if del_resp.hovered() {
                    colors::ERROR
                } else {
                    colors::TEXT_MUTED
                },
            );
            ui.painter().galley(
                del_rect.center() - galley.size() * 0.5,
                galley,
                colors::TEXT_MUTED,
            );
            if del_resp.clicked() {
                *to_delete = Some(card.id);
            }
        });
    });
}

// ── Footer ────────────────────────────────────────────────────────

fn footer(ui: &mut egui::Ui, state: &mut AppState) {
    let sep_rect = egui::Rect::from_min_size(
        egui::pos2(ui.min_rect().left(), ui.cursor().top()),
        egui::vec2(ui.available_width(), 1.0),
    );
    ui.painter()
        .rect_filled(sep_rect, CornerRadius::ZERO, colors::BORDER_WEAK);
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.colored_label(
            colors::TEXT_MUTED,
            egui::RichText::new("👤 makima").size(11.0),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("⚙  Settings")
                            .size(11.0)
                            .color(colors::TEXT_MUTED),
                    )
                    .fill(colors::TRANSPARENT)
                    .stroke(egui::Stroke::NONE)
                    .min_size(egui::vec2(60.0, 20.0)),
                )
                .on_hover_text("Open settings")
                .clicked()
            {
                state.show_settings_panel = true;
            }
        });
    });
}

// ── Helpers ───────────────────────────────────────────────────────

fn relative_time_str(dt: chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(dt);
    let mins = duration.num_minutes();
    let hours = duration.num_hours();
    let days = duration.num_days();

    if mins < 1 {
        "now".to_string()
    } else if mins < 60 {
        format!("{}m", mins)
    } else if hours < 24 {
        format!("{}h", hours)
    } else if days < 7 {
        format!("{}d", days)
    } else if days < 30 {
        format!("{}w", days / 7)
    } else {
        format!("{}mo", days / 30)
    }
}

fn truncate_label(_ui: &egui::Ui, s: &str, max_width: f32) -> String {
    if max_width <= 20.0 {
        return s.chars().take(2).collect::<String>() + "…";
    }

    let avg_px: f32 = 11.0;
    let max_chars = (max_width / avg_px).max(3.0) as usize;
    let count = s.chars().count();

    if count <= max_chars {
        return s.to_string();
    }

    let chars: Vec<char> = s.chars().collect();
    let mut result: String = chars[..max_chars.saturating_sub(1)].iter().collect();
    while !result.is_empty() && !s.starts_with(&result) {
        result.pop();
    }
    result + "…"
}