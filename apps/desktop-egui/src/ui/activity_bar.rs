use eframe::egui::{self, CornerRadius, Sense, Vec2};

use crate::state::app_state::{ActivitySection, AppState};
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.add_space(4.0);
        draw_brand_dot(ui);
        ui.add_space(18.0);

        activity_button(ui, state, ActivitySection::Sessions, ActivityIcon::Sessions, "Sessions");
        activity_button(ui, state, ActivitySection::Resources, ActivityIcon::Resources, "Resources");
        activity_button(ui, state, ActivitySection::Agent, ActivityIcon::Agent, "Agent");
        activity_button(
            ui,
            state,
            ActivitySection::Integrations,
            ActivityIcon::Integrations,
            "Integrations",
        );
    });
}

#[derive(Clone, Copy)]
enum ActivityIcon {
    Sessions,
    Resources,
    Agent,
    Integrations,
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
    icon: ActivityIcon,
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

    ui.painter()
        .rect_filled(rect, CornerRadius::same(10), bg);
    paint_icon(ui, rect, icon, active);

    if response.on_hover_text(tooltip).clicked() {
        state.activity_section = section;
    }

    ui.add_space(6.0);
}

fn paint_icon(ui: &egui::Ui, rect: egui::Rect, icon: ActivityIcon, active: bool) {
    let painter = ui.painter();
    let color = if active {
        colors::ICON_ACTIVE
    } else {
        colors::ICON_DEFAULT
    };
    let stroke = egui::Stroke::new(1.4, color);

    match icon {
        ActivityIcon::Sessions => {
            let bubble = egui::Rect::from_center_size(rect.center(), egui::vec2(14.0, 10.0));
            painter.rect_stroke(
                bubble,
                CornerRadius::same(4),
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.line_segment(
                [
                    egui::pos2(bubble.left() + 4.0, bubble.bottom()),
                    egui::pos2(bubble.left() + 7.0, bubble.bottom() + 3.0),
                ],
                stroke,
            );
        }
        ActivityIcon::Resources => {
            let top = egui::Rect::from_center_size(
                rect.center() + egui::vec2(0.0, -3.5),
                egui::vec2(12.0, 5.0),
            );
            let bottom = egui::Rect::from_center_size(
                rect.center() + egui::vec2(0.0, 4.5),
                egui::vec2(12.0, 5.0),
            );
            painter.rect_stroke(top, CornerRadius::same(2), stroke, egui::StrokeKind::Middle);
            painter.rect_stroke(
                bottom,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        ActivityIcon::Agent => {
            painter.circle_stroke(rect.center_top() + egui::vec2(0.0, 11.0), 3.5, stroke);
            painter.line_segment(
                [
                    rect.center() + egui::vec2(-6.0, 7.0),
                    rect.center() + egui::vec2(6.0, 7.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    rect.center() + egui::vec2(-4.0, 7.0),
                    rect.center() + egui::vec2(0.0, 1.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    rect.center() + egui::vec2(4.0, 7.0),
                    rect.center() + egui::vec2(0.0, 1.0),
                ],
                stroke,
            );
        }
        ActivityIcon::Integrations => {
            painter.circle_stroke(rect.center(), 4.0, stroke);
            painter.line_segment(
                [rect.center() + egui::vec2(-7.0, 0.0), rect.center() + egui::vec2(-4.0, 0.0)],
                stroke,
            );
            painter.line_segment(
                [rect.center() + egui::vec2(4.0, 0.0), rect.center() + egui::vec2(7.0, 0.0)],
                stroke,
            );
            painter.line_segment(
                [rect.center() + egui::vec2(0.0, -7.0), rect.center() + egui::vec2(0.0, -4.0)],
                stroke,
            );
            painter.line_segment(
                [rect.center() + egui::vec2(0.0, 4.0), rect.center() + egui::vec2(0.0, 7.0)],
                stroke,
            );
        }
    }
}
