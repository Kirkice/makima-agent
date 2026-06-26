//! Zoo-Code-inspired chat message bubbles.
//!
//! Each message type has its own dedicated component with distinct visual treatment:
//! - **User**: Right-aligned, subtle background, no accent bar
//! - **Assistant**: Icon header + left accent bar + body
//! - **Reasoning/Thinking**: Collapsible 💡 block with timer
//! - **Tool execution**: Card with tool name + collapsible output
//! - **Error**: Red border + ⚠ icon + expandable details

use eframe::egui::{self, Color32, CornerRadius};

use crate::state::chat_state::{ChatMessage, SayKind, TokenUsage};
use crate::theme::colors;

// ── Shared constants ─────────────────────────────────────────────────

const BUBBLE_RADIUS: f32 = 10.0;
const HEADER_ICON_SIZE: f32 = 14.0;
const HEADER_TEXT_SIZE: f32 = 12.0;
const BODY_TEXT_SIZE: f32 = 13.5;
const META_TEXT_SIZE: f32 = 11.0;
const ACCENT_BAR_WIDTH: f32 = 3.0;
const BUBBLE_PAD_X: f32 = 14.0;
const BUBBLE_PAD_Y: f32 = 10.0;

// ── Public: route to correct bubble ──────────────────────────────────

/// Draw the appropriate bubble for a chat message.
/// Returns text to copy if the user right-clicks → Copy.
pub fn draw_message(ui: &mut egui::Ui, msg: &ChatMessage, copy_text: &mut Option<String>) {
    let is_user = matches!(msg.msg_type, crate::state::chat_state::MessageType::Ask);

    if is_user {
        draw_user_bubble(ui, msg, copy_text);
    } else {
        match msg.say {
            Some(SayKind::Reasoning) => draw_reasoning_bubble(ui, msg, copy_text),
            Some(SayKind::Tool | SayKind::McpServerRequestStarted | SayKind::McpServerResponse) => {
                draw_tool_bubble(ui, msg, copy_text);
            }
            Some(SayKind::Error) => draw_error_bubble(ui, msg, copy_text),
            _ => draw_assistant_bubble(ui, msg, copy_text),
        }
    }
}

// ── User Bubble ──────────────────────────────────────────────────────
// Zoo-Code: right-aligned, no accent bar, subtle bg

fn draw_user_bubble(ui: &mut egui::Ui, msg: &ChatMessage, copy_text: &mut Option<String>) {
    let text = msg.text.clone().unwrap_or_default();
    let ts = format_timestamp(msg.ts);

    // Right-align: add spacer before
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
        let max_w = (ui.available_width() * 0.75).max(120.0);

        let response = egui::Frame::NONE
            .fill(colors::BUBBLE_USER_BG)
            .corner_radius(CornerRadius::same(BUBBLE_RADIUS as u8))
            .inner_margin(egui::Margin {
                left: BUBBLE_PAD_X as i8,
                right: BUBBLE_PAD_X as i8,
                top: BUBBLE_PAD_Y as i8,
                bottom: BUBBLE_PAD_Y as i8,
            })
            .show(ui, |ui| {
                ui.set_max_width(max_w);

                // Header: "You · timestamp"
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.colored_label(
                            colors::TEXT_MUTED,
                            egui::RichText::new(&ts).size(META_TEXT_SIZE),
                        );
                        ui.colored_label(
                            colors::RED_ACCENT,
                            egui::RichText::new("You").size(HEADER_TEXT_SIZE).strong(),
                        );
                    });
                });

                ui.add_space(4.0);

                // Body text
                ui.colored_label(
                    colors::TEXT_PRIMARY,
                    egui::RichText::new(&text).size(BODY_TEXT_SIZE),
                );
            });

        response.response.context_menu(|ui| {
            if ui.button("📋  Copy message").clicked() {
                *copy_text = Some(text.clone());
                ui.close();
            }
        });
    });
}

// ── Assistant Bubble ─────────────────────────────────────────────────
// Zoo-Code: icon header + left accent bar + body

fn draw_assistant_bubble(ui: &mut egui::Ui, msg: &ChatMessage, copy_text: &mut Option<String>) {
    let text = msg.text.clone().unwrap_or_default();

    let response = egui::Frame::NONE
        .fill(colors::BUBBLE_ASSISTANT_BG)
        .corner_radius(CornerRadius::same(BUBBLE_RADIUS as u8))
        .inner_margin(egui::Margin {
            left: BUBBLE_PAD_X as i8,
            right: BUBBLE_PAD_X as i8,
            top: BUBBLE_PAD_Y as i8,
            bottom: BUBBLE_PAD_Y as i8,
        })
        .show(ui, |ui| {
            // Left accent bar
            draw_accent_bar(ui, colors::TEXT_PRIMARY);

            // Header: 🤖 Makima · streaming?
            draw_header(ui, "🤖", "Makima", colors::TEXT_PRIMARY, msg.partial);

            ui.add_space(6.0);

            // Body
            draw_body_text(ui, &text);

            // Token usage footer
            if let Some(tok) = msg.token_usage {
                ui.add_space(6.0);
                draw_token_footer(ui, tok);
            }
        });

    response.response.context_menu(|ui| {
        if ui.button("📋  Copy message").clicked() {
            *copy_text = Some(text.clone());
            ui.close();
        }
    });
}

// ── Reasoning / Thinking Bubble ──────────────────────────────────────
// Zoo-Code: 💡 collapsible, timer, indented content with left gray bar

fn draw_reasoning_bubble(ui: &mut egui::Ui, msg: &ChatMessage, copy_text: &mut Option<String>) {
    let text = msg.text.clone().unwrap_or_default();

    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(BUBBLE_RADIUS as u8))
        .inner_margin(egui::Margin {
            left: BUBBLE_PAD_X as i8,
            right: BUBBLE_PAD_X as i8,
            top: 6,
            bottom: 6,
        })
        .show(ui, |ui| {
            // Collapsible header
            let id = ui.make_persistent_id(format!("reasoning_{}", msg.id));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
                .show_header(ui, |ui| {
                    // 💡 icon + "Thinking" + elapsed
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            Color32::from_rgb(245, 158, 11), // amber
                            egui::RichText::new("💡").size(HEADER_ICON_SIZE),
                        );
                        ui.colored_label(
                            colors::TEXT_PRIMARY,
                            egui::RichText::new("Thinking").size(HEADER_TEXT_SIZE).strong(),
                        );
                        if msg.partial {
                            ui.colored_label(
                                colors::TEXT_MUTED,
                                egui::RichText::new("...").size(META_TEXT_SIZE),
                            );
                        }
                    });
                })
                .body(|ui| {
                    ui.add_space(4.0);

                    // Left gray bar + indented text (Zoo-Code style)
                    ui.horizontal(|ui| {
                        // Gray accent bar
                        let bar_rect = egui::Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(2.0, 0.0), // height will be set after content
                        );
                        ui.add_space(2.0);

                        // Content
                        ui.vertical(|ui| {
                            ui.add_space(2.0);
                            ui.colored_label(
                                colors::TEXT_SECONDARY,
                                egui::RichText::new(&text)
                                    .size(BODY_TEXT_SIZE)
                                    .italics(),
                            );
                            ui.add_space(2.0);
                        });

                        // Draw bar matching content height
                        let content_height = ui.min_rect().height();
                        let full_bar = egui::Rect::from_min_size(
                            bar_rect.min,
                            egui::vec2(2.0, content_height),
                        );
                        ui.painter()
                            .rect_filled(full_bar, CornerRadius::same(1), colors::TEXT_MUTED);
                    });
                });
        });

    // Context menu on the outer frame area
    let _ = copy_text; // copy not critical for reasoning
}

// ── Tool Execution Bubble ────────────────────────────────────────────
// Zoo-Code: card with tool name header + collapsible output

fn draw_tool_bubble(ui: &mut egui::Ui, msg: &ChatMessage, copy_text: &mut Option<String>) {
    let text = msg.text.clone().unwrap_or_default();
    let tool_name = extract_tool_name(&text);

    let response = egui::Frame::NONE
        .fill(colors::BUBBLE_TOOL_BG)
        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(35, 50, 72)))
        .corner_radius(CornerRadius::same(BUBBLE_RADIUS as u8))
        .inner_margin(egui::Margin {
            left: BUBBLE_PAD_X as i8,
            right: BUBBLE_PAD_X as i8,
            top: BUBBLE_PAD_Y as i8,
            bottom: BUBBLE_PAD_Y as i8,
        })
        .show(ui, |ui| {
            // Left accent bar (blue)
            draw_accent_bar(ui, colors::INFO);

            // Header: ⚙ tool_name
            ui.horizontal(|ui| {
                ui.colored_label(
                    colors::INFO,
                    egui::RichText::new("⚙").size(HEADER_ICON_SIZE),
                );
                ui.colored_label(
                    colors::TEXT_PRIMARY,
                    egui::RichText::new(&tool_name).size(HEADER_TEXT_SIZE).strong(),
                );
                if msg.partial {
                    ui.add_space(6.0);
                    ui.spinner();
                }
            });

            ui.add_space(6.0);

            // Collapsible output — try diff view first, fallback to terminal
            let output_text = extract_tool_output(&text);
            if !output_text.is_empty() {
                // Check if output looks like a unified diff
                let is_diff = output_text.contains("diff --git")
                    || (output_text.contains("@@ ") && output_text.contains("--- ") && output_text.contains("+++ "));

                let id = ui.make_persistent_id(format!("tool_out_{}", msg.id));
                egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    false,
                )
                .show_header(ui, |ui| {
                    let label = if is_diff { "Diff" } else { "Output" };
                    ui.colored_label(
                        colors::TEXT_MUTED,
                        egui::RichText::new(label).size(META_TEXT_SIZE),
                    );
                })
                .body(|ui| {
                    ui.add_space(4.0);

                    if is_diff {
                        // Parse and render unified diff view
                        let changes = super::diff_view::parse_unified_diff(&output_text);
                        super::diff_view::draw_file_changes(ui, &changes);
                    } else {
                        // Terminal-style output: dark bg + monospace
                        egui::Frame::NONE
                            .fill(Color32::from_rgb(13, 17, 23))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(egui::Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                let display = if output_text.len() > 2000 {
                                    format!("{}...", &output_text[..2000])
                                } else {
                                    output_text.clone()
                                };
                                ui.colored_label(
                                    Color32::from_rgb(180, 220, 180),
                                    egui::RichText::new(&display)
                                        .size(12.0)
                                        .family(egui::FontFamily::Monospace),
                                );
                            });
                    }
                });
            }
        });

    response.response.context_menu(|ui| {
        if ui.button("📋  Copy output").clicked() {
            *copy_text = Some(text.clone());
            ui.close();
        }
    });
}

// ── Error Bubble ─────────────────────────────────────────────────────
// Zoo-Code: red border + ⚠ icon

fn draw_error_bubble(ui: &mut egui::Ui, msg: &ChatMessage, copy_text: &mut Option<String>) {
    let text = msg.text.clone().unwrap_or_default();
    let err_detail = msg.error.clone().unwrap_or_default();

    let response = egui::Frame::NONE
        .fill(Color32::from_rgb(57, 28, 32))
        .stroke(egui::Stroke::new(1.0, colors::ERROR))
        .corner_radius(CornerRadius::same(BUBBLE_RADIUS as u8))
        .inner_margin(egui::Margin {
            left: BUBBLE_PAD_X as i8,
            right: BUBBLE_PAD_X as i8,
            top: BUBBLE_PAD_Y as i8,
            bottom: BUBBLE_PAD_Y as i8,
        })
        .show(ui, |ui| {
            // Left accent bar (red)
            draw_accent_bar(ui, colors::ERROR);

            // Header: ✖ Error
            draw_header(ui, "✖", "Error", colors::ERROR, false);

            ui.add_space(6.0);

            // Error message body
            if !text.is_empty() {
                ui.colored_label(
                    colors::TEXT_PRIMARY,
                    egui::RichText::new(&text).size(BODY_TEXT_SIZE),
                );
            }

            if !err_detail.is_empty() {
                ui.add_space(4.0);
                ui.colored_label(
                    colors::ERROR,
                    egui::RichText::new(&err_detail).size(12.0),
                );
            }
        });

    response.response.context_menu(|ui| {
        if ui.button("📋  Copy error").clicked() {
            *copy_text = Some(format!("{}\n{}", text, err_detail));
            ui.close();
        }
    });
}

// ── Shared helpers ───────────────────────────────────────────────────

fn draw_accent_bar(ui: &mut egui::Ui, color: Color32) {
    // Draw a thin bar on the left edge of the current frame
    let frame_rect = ui.min_rect();
    let bar_rect = egui::Rect::from_min_size(
        frame_rect.min,
        egui::vec2(ACCENT_BAR_WIDTH, frame_rect.height()),
    );
    ui.painter()
        .rect_filled(bar_rect, CornerRadius::same(2), color);
}

fn draw_header(ui: &mut egui::Ui, icon: &str, title: &str, color: Color32, streaming: bool) {
    ui.horizontal(|ui| {
        ui.colored_label(
            color,
            egui::RichText::new(icon).size(HEADER_ICON_SIZE),
        );
        ui.add_space(4.0);
        ui.colored_label(
            color,
            egui::RichText::new(title).size(HEADER_TEXT_SIZE).strong(),
        );
        if streaming {
            ui.add_space(8.0);
            ui.colored_label(
                colors::WARNING,
                egui::RichText::new("◌ streaming").size(META_TEXT_SIZE),
            );
        }
    });
}

fn draw_body_text(ui: &mut egui::Ui, text: &str) {
    use super::markdown_render;

    if text.len() > 600 {
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                markdown_render::draw_markdown(ui, text);
            });
    } else {
        markdown_render::draw_markdown(ui, text);
    }
}

fn draw_token_footer(ui: &mut egui::Ui, tok: TokenUsage) {
    let usage_text = format!(
        "↑ {}  ↓ {}  ·  ${:.5}",
        format_tokens(tok.total_tokens_in),
        format_tokens(tok.total_tokens_out),
        tok.total_cost,
    );
    ui.colored_label(
        colors::TEXT_MUTED,
        egui::RichText::new(usage_text).size(META_TEXT_SIZE),
    );
}

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1000 {
        format!("{:.1}K", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

fn format_timestamp(ts_millis: i64) -> String {
    use chrono::TimeZone;
    let dt = chrono::Local.timestamp_millis_opt(ts_millis);
    match dt {
        chrono::LocalResult::Single(dt) => dt.format("%H:%M").to_string(),
        _ => String::new(),
    }
}

/// Extract tool name from message text.
/// Tool messages often look like "✅ read_file: ..." or just contain the tool name.
fn extract_tool_name(text: &str) -> String {
    // Try to find "tool_name" pattern after common prefixes
    let cleaned = text
        .trim_start_matches("✅ ")
        .trim_start_matches("❌ ")
        .trim_start_matches("⚙ ")
        .trim();

    // Take first word or up to first colon/space
    let name_end = cleaned
        .find(|c: char| c == ':' || c == ' ' || c == '\n')
        .unwrap_or(cleaned.len().min(40));

    if name_end > 0 {
        cleaned[..name_end].to_string()
    } else {
        "Tool".to_string()
    }
}

/// Extract tool output from message text (everything after the first colon or newline).
fn extract_tool_output(text: &str) -> String {
    if let Some(pos) = text.find(':') {
        let rest = text[pos + 1..].trim();
        if rest.is_empty() {
            text.to_string()
        } else {
            rest.to_string()
        }
    } else {
        String::new()
    }
}
