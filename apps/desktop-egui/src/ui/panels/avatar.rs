use eframe::egui::{self, CornerRadius};

use crate::state::app_state::AppState;
use crate::theme::colors;

const PIXEL_COLORS: [[Option<&str>; 13]; 15] = [
    [None, None, None, Some("#b94d58"), Some("#bc4f5a"), Some("#d05966"), Some("#d15966"), Some("#d15966"), Some("#d05966"), Some("#bc4f5a"), Some("#b94d58"), None, None],
    [None, None, Some("#d25864"), Some("#ce5964"), Some("#d15966"), Some("#d25a67"), Some("#d15a67"), Some("#d15a67"), Some("#d25a67"), Some("#d15966"), Some("#ce5964"), Some("#d25864"), None],
    [None, Some("#c1515a"), Some("#cf5865"), Some("#d15966"), Some("#d25b67"), Some("#d05966"), Some("#d15b67"), Some("#d15b67"), Some("#d05966"), Some("#d25b67"), Some("#d15966"), Some("#cf5865"), Some("#c1515a")],
    [None, Some("#c45862"), Some("#d05863"), Some("#d05964"), Some("#d15663"), Some("#d15966"), Some("#d15a67"), Some("#d15a67"), Some("#d15966"), Some("#d15663"), Some("#d05964"), Some("#d05863"), Some("#c45862")],
    [None, Some("#d15965"), Some("#d05965"), Some("#d05966"), Some("#d85f6a"), Some("#a13e4a"), Some("#d25b68"), Some("#d25b68"), Some("#a13e4a"), Some("#d85f6a"), Some("#d05966"), Some("#d05965"), Some("#d15965")],
    [Some("#ae4350"), Some("#d15a66"), Some("#a53f4c"), Some("#cf5865"), Some("#ca4d5d"), Some("#a33e4b"), Some("#d25a67"), Some("#d25a67"), Some("#a33e4b"), Some("#ca4d5d"), Some("#cf5865"), Some("#a53f4c"), Some("#d15a66")],
    [Some("#b74957"), Some("#b14552"), Some("#a33d4a"), Some("#ae4654"), Some("#ab3b48"), Some("#e0afa3"), Some("#bf4c59"), Some("#bf4c59"), Some("#e0afa3"), Some("#ab3b48"), Some("#ae4654"), Some("#a33d4a"), Some("#b14552")],
    [Some("#b64856"), Some("#ba4d58"), Some("#8f554e"), Some("#fcebdd"), Some("#fcebdd"), Some("#fcecde"), Some("#fdebde"), Some("#fdebde"), Some("#fcecde"), Some("#fcebdd"), Some("#fcebdd"), Some("#8f554e"), Some("#ba4d58")],
    [Some("#b64956"), Some("#b44b56"), Some("#4e2a15"), Some("#45210e"), Some("#42200e"), Some("#975d52"), Some("#fceadc"), Some("#fceadc"), Some("#975d52"), Some("#42200e"), Some("#45210e"), Some("#4e2a15"), Some("#b44b56")],
    [Some("#b74b58"), Some("#b54b56"), Some("#814d43"), Some("#fefbfc"), Some("#f8c819"), Some("#fdf7f0"), Some("#fdeadd"), Some("#fdeadd"), Some("#fdf7f0"), Some("#f8c819"), Some("#fefbfc"), Some("#814d43"), Some("#b54b56")],
    [Some("#b54b56"), Some("#b14953"), Some("#b74b58"), Some("#fceade"), Some("#fde9d7"), Some("#fceadd"), Some("#fceadc"), Some("#fceadc"), Some("#fceadd"), Some("#fde9d7"), Some("#fceade"), Some("#b74b58"), Some("#b14953")],
    [None, Some("#a7424f"), Some("#a4424e"), Some("#fbeadd"), Some("#fbebdd"), Some("#fdebdd"), Some("#af6f6a"), Some("#af6f6a"), Some("#fdebdd"), Some("#fbebdd"), Some("#fbeadd"), Some("#a4424e"), Some("#a7424f")],
    [None, Some("#a7414d"), None, Some("#834e66"), Some("#f8ded4"), Some("#f9e4d9"), Some("#ecc7ba"), Some("#ecc7ba"), Some("#f9e4d9"), Some("#f8ded4"), Some("#834e66"), None, Some("#a7414d")],
    [None, Some("#b34858"), None, None, Some("#bcbab4"), Some("#f6f2e8"), Some("#f7f3eb"), Some("#f7f3eb"), Some("#f6f2e8"), Some("#bcbab4"), None, None, Some("#b34858")],
    [None, Some("#9b4d62"), None, Some("#969394"), Some("#f8f3ea"), Some("#bbb9b2"), Some("#3e393b"), Some("#3e393b"), Some("#bbb9b2"), Some("#f8f3ea"), Some("#969394"), None, Some("#9b4d62")],
];

pub fn draw(ui: &mut egui::Ui, state: &AppState) {
    egui::Frame::NONE
        .fill(colors::SURFACE)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(20))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                // Header
                ui.colored_label(
                    colors::TEXT_PRIMARY,
                    egui::RichText::new("👤  Avatar Stage").size(20.0).strong(),
                );
                ui.colored_label(
                    colors::TEXT_MUTED,
                    egui::RichText::new("3D Avatar rendering — Unity WebView").size(13.0),
                );
                ui.add_space(24.0);

                // ── Pixel art avatar ──
                let pixel_size = 14.0;
                let avatar_size = egui::vec2(13.0 * pixel_size, 15.0 * pixel_size);

                // Background frame for the avatar
                let frame_pad = 16.0;
                let frame_size = avatar_size + egui::vec2(frame_pad * 2.0, frame_pad * 2.0);
                let (frame_rect, _) =
                    ui.allocate_exact_size(frame_size, egui::Sense::hover());

                // Draw rounded bg behind avatar
                ui.painter_at(frame_rect).rect_filled(
                    frame_rect,
                    CornerRadius::same(12),
                    colors::ELEVATED,
                );

                // Draw pixel art
                let art_rect = egui::Rect::from_min_size(
                    frame_rect.min + egui::vec2(frame_pad, frame_pad),
                    avatar_size,
                );

                let painter = ui.painter_at(art_rect);
                for (row_idx, row) in PIXEL_COLORS.iter().enumerate() {
                    for (col_idx, cell) in row.iter().enumerate() {
                        if let Some(hex) = cell {
                            let color = parse_hex_color(hex);
                            let min = egui::pos2(
                                art_rect.min.x + col_idx as f32 * pixel_size,
                                art_rect.min.y + row_idx as f32 * pixel_size,
                            );
                            let max = egui::pos2(min.x + pixel_size, min.y + pixel_size);
                            painter.rect_filled(
                                egui::Rect::from_min_max(min, max),
                                egui::CornerRadius::ZERO,
                                color,
                            );
                        }
                    }
                }

                ui.add_space(24.0);

                // ── Status strips ──
                kv_strip(
                    ui,
                    "🖥  Render",
                    "Unity WebGL",
                    colors::TEXT_MUTED,
                );

                let (voice_label, voice_color) = if state.voice_call.is_connected {
                    ("●  Connected", colors::SUCCESS)
                } else if state.voice_call.is_connecting {
                    ("◌  Connecting", colors::WARNING)
                } else {
                    ("○  Idle", colors::TEXT_MUTED)
                };
                kv_strip(ui, "🎙 Voice", voice_label, voice_color);

                kv_strip(
                    ui,
                    "🎯  Target",
                    "Unity WebView",
                    colors::INFO,
                );
            });
        });
}

fn kv_strip(ui: &mut egui::Ui, label: &str, value: &str, accent: egui::Color32) {
    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin {
            left: 12,
            right: 12,
            top: 9,
            bottom: 9,
        })
        .show(ui, |ui| {
            // Left accent bar
            let bar_rect = egui::Rect::from_min_size(
                ui.min_rect().min,
                egui::vec2(3.0, ui.min_rect().height()),
            );
            ui.painter()
                .rect_filled(bar_rect, CornerRadius::same(2), accent);

            ui.horizontal(|ui| {
                ui.colored_label(colors::TEXT_SECONDARY, label);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(
                        accent,
                        egui::RichText::new(value).size(13.0),
                    );
                });
            });
        });
    ui.add_space(6.0);
}

fn parse_hex_color(hex: &str) -> egui::Color32 {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    egui::Color32::from_rgb(r, g, b)
}