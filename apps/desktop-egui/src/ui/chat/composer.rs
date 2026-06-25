use crate::app::UiAction;
use crate::state::app_state::AppState;
use crate::state::chat_state::{AttachedFile, AttachmentStatus};
use crate::theme::colors;
use eframe::egui::{self, CornerRadius, Key};

/// Zoo-Code-style input panel.
///
/// Layout:
/// ┌──────────────────────────────────────────┐
/// │ 📎 report.pdf [✕]  photo.png [✕]        │  ← attachments (if any)
/// │                                          │
/// │  [auto-resizing textarea, min 2 rows]    │  ← main input
/// │                                          │
/// ├──────────────────────────────────────────┤
/// │ 🛠️ Code ▼  📎 Attach     ~128 tok  ⬆️   │  ← single toolbar row
/// └──────────────────────────────────────────┘
pub fn draw(
    ui: &mut egui::Ui,
    state: &mut AppState,
    pending_action: &mut Option<UiAction>,
) -> bool {
    let mut should_send = false;

    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK.linear_multiply(0.5)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            // Fill the entire Dock-allocated region
            ui.set_min_height(ui.available_height());
            // ── Attachment previews ──────────────────────────────
            if !state.chat.composer.attachments.is_empty() {
                draw_attachment_previews(ui, state);
                ui.add_space(4.0);
            }

            // ── Main text area ───────────────────────────────────
            let text_response = ui.add_sized(
                egui::vec2(ui.available_width(), 0.0),
                egui::TextEdit::multiline(&mut state.chat.composer.input)
                    .hint_text("Ask Makima anything... (Enter to send, Shift+Enter for newline)")
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .frame(false),
            );

            // Push toolbar to the bottom with a flexible spacer
            let remaining = ui.available_height().max(0.0);
            if remaining > 32.0 {
                ui.add_space(remaining - 28.0);
            }

            // ── Single bottom toolbar (Zoo-Code style) ───────────
            draw_toolbar(ui, state, pending_action, &mut should_send);

            // ── Keyboard: Enter = send, Shift+Enter = newline ────
            let enter_pressed = ui.input(|i| {
                i.key_pressed(Key::Enter) && !i.modifiers.shift && !i.modifiers.ctrl
            });
            let ctrl_enter = ui.input(|i| {
                i.key_pressed(Key::Enter) && i.modifiers.ctrl
            });

            if (enter_pressed || ctrl_enter)
                && !state.chat.composer.input.trim().is_empty()
                && !state.chat.composer.is_streaming
            {
                should_send = true;
                text_response.request_focus();
            }
        });

    should_send
}

// ── Attachment Previews (compact, Zoo-Code style) ─────────────────────

fn draw_attachment_previews(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal_wrapped(|ui| {
        let mut to_remove: Vec<usize> = Vec::new();

        for (idx, file) in state.chat.composer.attachments.iter().enumerate() {
            let status_icon = match file.status {
                AttachmentStatus::Pending => "⏳",
                AttachmentStatus::Uploading => "⬆️",
                AttachmentStatus::Uploaded => "✅",
                AttachmentStatus::Error(_) => "❌",
            };

            let size_str = if file.size > 1_000_000 {
                format!("{:.1}MB", file.size as f64 / 1_000_000.0)
            } else if file.size > 1000 {
                format!("{:.1}KB", file.size as f64 / 1000.0)
            } else {
                format!("{}B", file.size)
            };

            let text = format!("📎 {:.30} ({})", file.name, size_str);
            let label = egui::Label::new(
                egui::RichText::new(format!("{} {}", status_icon, text)).size(11.0),
            );

            let frame = egui::Frame::NONE
                .fill(colors::SURFACE)
                .corner_radius(CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(6, 2));

            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(label);
                    if ui
                        .add(
                            egui::Button::new("✕")
                                .fill(colors::TRANSPARENT)
                                .stroke(egui::Stroke::NONE)
                                .min_size(egui::vec2(16.0, 16.0)),
                        )
                        .on_hover_text("Remove")
                        .clicked()
                    {
                        to_remove.push(idx);
                    }
                });
            });

            ui.add_space(3.0);
        }

        for idx in to_remove.into_iter().rev() {
            state.chat.composer.attachments.remove(idx);
        }
    });
}

// ── Bottom Toolbar (single row, Zoo-Code proportions) ──────────────────

fn draw_toolbar(
    ui: &mut egui::Ui,
    state: &mut AppState,
    pending_action: &mut Option<UiAction>,
    should_send: &mut bool,
) {
    let narrow = ui.available_width() < 420.0;

    if narrow {
        // Narrow: stack in two rows
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                mode_dropdown(ui, state);
                ui.add_space(4.0);
                attach_btn(ui, state);
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                status_labels(ui, state);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    send_or_stop(ui, state, pending_action, should_send);
                });
            });
            ui.horizontal(|ui| {
                auto_approve_btn(ui, state);
            });
        });
    } else {
        // Normal: single horizontal row, all items vertically centered
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.horizontal(|ui| {
            // Left side: mode + attach + auto-approve
            ui.add_space(1.0);
            mode_dropdown(ui, state);
            ui.add_space(4.0);
            attach_btn(ui, state);
            ui.add_space(4.0);
            auto_approve_btn(ui, state);

            // Right side: char count + token count + send
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                send_or_stop(ui, state, pending_action, should_send);
                ui.add_space(6.0);
                status_labels(ui, state);
            });
        });
    }
}

fn mode_dropdown(ui: &mut egui::Ui, state: &mut AppState) {
    let current_name = state
        .settings
        .active_mode()
        .map(|m| compact_emoji_name(&m.name))
        .unwrap_or_else(|| "🛠️Code".to_string());

    // Use a local copy of slugs to avoid borrow issues inside ComboBox
    let modes: Vec<(String, String)> = state
        .settings
        .modes
        .iter()
        .map(|m| (m.slug.clone(), compact_emoji_name(&m.name)))
        .collect();
    let active_slug = state.settings.active_mode_slug.clone();
    let mut new_slug: Option<String> = None;

    let _response = egui::ComboBox::from_id_salt("composer_mode")
        .selected_text(&current_name)
        .width(130.0)
        .show_ui(ui, |ui| {
            for (slug, name) in &modes {
                let selected = Some(slug.clone()) == active_slug;
                let resp = ui.selectable_label(selected, name);
                if resp.clicked() && !selected {
                    new_slug = Some(slug.clone());
                }
            }
        });

    // Also check if ComboBox response itself indicates a change
    if let Some(slug) = new_slug {
        state.settings.active_mode_slug = Some(slug.clone());
        state.set_status(format!("Mode switched to {}", slug));
    }
}

fn attach_btn(ui: &mut egui::Ui, state: &mut AppState) {
    if ui
        .add(
            egui::Button::new("📎")
                .fill(colors::TRANSPARENT)
                .stroke(egui::Stroke::NONE)
                .min_size(egui::vec2(28.0, 24.0)),
        )
        .on_hover_text("Attach files (images, PDFs, code)")
        .clicked()
    {
        let files = rfd::FileDialog::new()
            .add_filter(
                "Documents & Images",
                &[
                    "pdf", "png", "jpg", "jpeg", "gif", "webp", "txt", "md", "py", "rs",
                    "toml", "yaml", "json", "csv", "html", "css", "js", "ts",
                ],
            )
            .add_filter("All Files", &["*"])
            .pick_files();

        if let Some(paths) = files {
            for path in paths {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let path_str = path.to_string_lossy().to_string();
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

                state.chat.composer.attachments.push(AttachedFile {
                    name,
                    path: path_str,
                    size,
                    status: AttachmentStatus::Pending,
                });
            }
        }
    }
}

fn auto_approve_btn(ui: &mut egui::Ui, state: &mut AppState) {
    let label = if state.chat.composer.auto_approve {
        "◉ Auto-Approve"
    } else {
        "○ Auto-Approve"
    };
    if state.chat.composer.auto_approve {
        colors::SUCCESS
    } else {
        colors::TEXT_MUTED
    };

    if ui
        .add_sized(
            egui::vec2(120.0, 20.0),
            egui::Button::new(label)
                .fill(colors::TRANSPARENT)
                .stroke(egui::Stroke::NONE),
        )
        .on_hover_text("Toggle auto-approve for tool execution")
        .clicked()
    {
        state.chat.composer.auto_approve = !state.chat.composer.auto_approve;
    }
}

fn status_labels(ui: &mut egui::Ui, state: &AppState) {
    let chars = state.chat.composer.input.len();
    let tokens = chars.div_ceil(4) as u64;

    if state.chat.composer.is_streaming {
        ui.colored_label(
            colors::WARNING,
            egui::RichText::new("◌ Streaming").size(11.0),
        );
    } else if chars > 0 {
        let cost = (tokens as f64 / 1000.0) * state.settings.token_estimate_per_1k;
        ui.colored_label(
            colors::TEXT_MUTED,
            egui::RichText::new(format!("📊 {} chars  ~{tokens} tok", chars)).size(11.0),
        );
        ui.add_space(4.0);
        ui.colored_label(
            colors::TEXT_MUTED,
            egui::RichText::new(format!("${cost:.4}")).size(11.0),
        );
    }
}

/// Remove the space right after an emoji so names like "🛠️ Code" → "🛠️Code"
fn compact_emoji_name(name: &str) -> String {
    // Find the position right after an emoji (first non-ASCII character)
    if let Some(idx) = name.find(|c: char| c.is_alphabetic()) {
        if idx > 0 {
            let emoji_part = name[..idx].trim_end();
            let text_part = &name[idx..];
            return format!("{}{}", emoji_part, text_part);
        }
    }
    name.to_string()
}

fn send_or_stop(
    ui: &mut egui::Ui,
    state: &mut AppState,
    pending_action: &mut Option<UiAction>,
    should_send: &mut bool,
) {
    if state.chat.composer.is_streaming {
        if ui
            .add(
                egui::Button::new("⏹")
                    .fill(colors::ERROR)
                    .stroke(egui::Stroke::NONE)
                    .min_size(egui::vec2(32.0, 24.0)),
            )
            .on_hover_text("Stop generating")
            .clicked()
        {
            state.chat.composer.is_streaming = false;
            *pending_action = None;
        }
    } else {
        let has_content = !state.chat.composer.input.trim().is_empty()
            || !state.chat.composer.attachments.is_empty();
        let can_send = has_content && state.is_logged_in;

        let btn = egui::Button::new("⬆")
            .fill(if can_send {
                colors::RED_ACCENT
            } else {
                colors::GRAPHITE_BORDER
            })
            .stroke(egui::Stroke::NONE)
            .min_size(egui::vec2(32.0, 24.0));

        if ui
            .add_enabled(can_send, btn)
            .on_hover_text(if can_send { "Send (Enter)" } else { "Type something to send" })
            .clicked()
        {
            *should_send = true;
            *pending_action = Some(UiAction::SendMessage);
        }
    }
}