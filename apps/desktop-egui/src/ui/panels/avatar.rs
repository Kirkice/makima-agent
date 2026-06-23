//! Avatar panel — 3D character rendering placeholder.
//!
//! Currently shows a placeholder with the pixel avatar and status.
//! In the future, this will embed a WebView (wry) running Unity WebGL
//! for real-time 3D character rendering with lip sync and emotions.

use crate::state::app_state::AppState;
use crate::theme::colors;
use eframe::egui;

/// Pixel art avatar (same as CLI)
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

/// Draw the avatar panel.
pub fn draw(ui: &mut egui::Ui, state: &AppState) {
    ui.vertical_centered(|ui| {
        ui.add_space(20.0);

        // Title
        ui.colored_label(colors::RED_ACCENT, egui::RichText::new("MAKIMA").size(24.0).strong());
        ui.add_space(4.0);
        ui.colored_label(colors::TEXT_SECONDARY, "3D Avatar View");

        ui.add_space(24.0);

        // Pixel avatar (scaled up)
        let pixel_size = 12.0;
        let avatar_size = egui::vec2(13.0 * pixel_size, 15.0 * pixel_size);
        let (rect, _) = ui.allocate_exact_size(avatar_size, egui::Sense::hover());

        let painter = ui.painter_at(rect);
        for (row_idx, row) in PIXEL_COLORS.iter().enumerate() {
            for (col_idx, cell) in row.iter().enumerate() {
                if let Some(hex) = cell {
                    let color = parse_hex_color(hex);
                    let min = egui::pos2(
                        rect.min.x + col_idx as f32 * pixel_size,
                        rect.min.y + row_idx as f32 * pixel_size,
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

        // Status section
        egui::Frame::NONE
            .fill(colors::GRAPHITE_ELEVATED)
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_min_width(200.0);
                ui.colored_label(colors::TEXT_SECONDARY, "Rendering Engine");
                ui.colored_label(colors::TEXT_PRIMARY, "Placeholder (egui pixel art)");
                ui.add_space(8.0);
                ui.colored_label(colors::TEXT_SECONDARY, "Target");
                ui.colored_label(colors::TEXT_PRIMARY, "Unity WebGL via wry");
                ui.add_space(8.0);

                let vc = &state.voice_call;
                let (status_icon, status_text) = if vc.is_connected {
                    ("●", colors::SUCCESS)
                } else if vc.is_connecting {
                    ("◌", colors::WARNING)
                } else {
                    ("○", colors::TEXT_MUTED)
                };
                ui.horizontal(|ui| {
                    ui.colored_label(colors::TEXT_SECONDARY, "Voice");
                    ui.colored_label(status_text, format!("{} {}", status_icon, vc.status));
                });
            });

        ui.add_space(16.0);

        // Emotion preview (placeholder)
        ui.colored_label(colors::TEXT_MUTED, "Emotion: Neutral");
        ui.colored_label(colors::TEXT_MUTED, "Lip Sync: Idle");

        ui.add_space(20.0);

        // Info text
        egui::Frame::NONE
            .fill(colors::GRAPHITE_BG)
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_min_width(220.0);
                ui.colored_label(
                    colors::TEXT_MUTED,
                    egui::RichText::new("ℹ This panel will display a 3D avatar\nrendered by Unity WebGL when the\n'avatar' feature is enabled.")
                        .size(11.0),
                );
                ui.add_space(4.0);
                ui.colored_label(
                    colors::TEXT_MUTED,
                    egui::RichText::new("cargo build --features avatar")
                        .monospace()
                        .size(10.0)
                        .color(colors::RED_DIM),
                );
            });
    });
}

fn parse_hex_color(hex: &str) -> egui::Color32 {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    egui::Color32::from_rgb(r, g, b)
}