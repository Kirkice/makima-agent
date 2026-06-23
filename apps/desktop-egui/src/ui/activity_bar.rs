use eframe::egui::{self, CornerRadius, Sense, Vec2};

use crate::state::app_state::{ActivitySection, AppState};
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.vertical_centered(|ui| {
        ui.add_space(4.0);
        draw_brand_dot(ui);
        ui.add_space(18.0);

        activity_button(ui, state, ActivitySection::Sessions, "💬", "Sessions");
        activity_button(ui, state, ActivitySection::Resources, "📚", "Resources");
        activity_button(ui, state, ActivitySection::Agent, "◈", "Agent");
        activity_button(ui, state, ActivitySection::Integrations, "🔌", "Integrations");
    });
}

fn draw_brand_dot(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 4.0, colors::RED_ACCENT);
}

fn activity_button(
    ui: &mut egui::Ui,
    state: &mut AppState,
    section: ActivitySection,
    icon: &str,
    tooltip: &str,
) {
    let active = state.activity_section == section;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::click());

    let bg = if active {
        colors::SELECTION_SOFT
    } else if response.hovered() {
        colors::ELEVATED
    } else {
        colors::TRANSPARENT
    };

    let fg = if active {
        colors::ICON_ACTIVE
    } else {
        colors::ICON_DEFAULT
    };

    ui.painter()
        .rect_filled(rect, CornerRadius::same(10), bg);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(16.0),
        fg,
    );

    if response.on_hover_text(tooltip).clicked() {
        state.activity_section = section;
    }

    ui.add_space(6.0);
}
