use eframe::egui::{self, CornerRadius, FontId, Sense, Vec2};

use crate::state::app_state::AppState;
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, _state: &mut AppState) {
    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.add_space(4.0);
        draw_brand_dot(ui);
        ui.add_space(18.0);

        // Only Sessions button — all other sections moved to Settings panel
        sessions_button(ui, "💬", "Conversations");
    });
}

fn draw_brand_dot(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 4.0, colors::RED_ACCENT);
}

fn sessions_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::click());

    let bg = if response.hovered() {
        colors::ELEVATED
    } else {
        colors::TRANSPARENT
    };

    ui.painter()
        .rect_filled(rect, CornerRadius::same(10), bg);

    // Always show as active — this is the only button
    let icon_size = 18.0;
    let icon_color = colors::ICON_ACTIVE;
    let galley = ui.painter().layout_no_wrap(
        icon.to_string(),
        FontId::proportional(icon_size),
        icon_color,
    );
    ui.painter().galley(
        rect.center() - galley.size() * 0.5,
        galley,
        icon_color,
    );

    if response.on_hover_text(tooltip).clicked() {
        // No-op — sessions is always shown now
    }

    ui.add_space(6.0);
}