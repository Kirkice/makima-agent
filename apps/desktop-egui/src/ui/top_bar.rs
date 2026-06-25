use eframe::egui::{self, CornerRadius};

use crate::state::app_state::{AppState, ViewMode};
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if state.is_logged_in {
            tool_button(
                ui,
                ToolIcon::Settings(state.show_settings_panel),
                "Toggle Settings",
                || {
                    state.show_settings_panel = !state.show_settings_panel;
                },
            );
            ui.add_space(8.0);

            tool_button(
                ui,
                ToolIcon::Avatar(state.view_mode == ViewMode::Avatar),
                "Avatar",
                || {
                    state.view_mode = ViewMode::Avatar;
                },
            );
            ui.add_space(8.0);

            tool_button(
                ui,
                ToolIcon::Chat(state.view_mode == ViewMode::Chat),
                "Chat",
                || {
                    state.view_mode = ViewMode::Chat;
                },
            );
        }
    });
}

#[derive(Clone, Copy)]
enum ToolIcon {
    Chat(bool),
    Avatar(bool),
    Settings(bool),
}

fn tool_button<F: FnOnce()>(ui: &mut egui::Ui, icon: ToolIcon, tooltip: &str, on_click: F) {
    let mut callback = Some(on_click);

    let active = match icon {
        ToolIcon::Chat(active) | ToolIcon::Avatar(active) | ToolIcon::Settings(active) => active,
    };

    let button = egui::Button::new("")
        .min_size(egui::vec2(30.0, 30.0))
        .fill(if active {
            colors::SELECTION_SOFT
        } else {
            colors::ELEVATED
        })
        .corner_radius(CornerRadius::same(10))
        .stroke(egui::Stroke::NONE);

    let response = ui.add(button).on_hover_text(tooltip);
    paint_icon(ui, response.rect, icon);

    if response.clicked() {
        if let Some(cb) = callback.take() {
            cb();
        }
    }
}

fn paint_icon(ui: &egui::Ui, rect: egui::Rect, icon: ToolIcon) {
    let painter = ui.painter();
    let stroke = egui::Stroke::new(1.4, colors::TEXT_PRIMARY);

    match icon {
        ToolIcon::Chat(_) => {
            let bubble = egui::Rect::from_center_size(rect.center(), egui::vec2(14.0, 10.0));
            painter.rect_stroke(
                bubble,
                CornerRadius::same(4),
                stroke,
                egui::StrokeKind::Middle,
            );
            let tail = [
                egui::pos2(bubble.left() + 3.0, bubble.bottom()),
                egui::pos2(bubble.left() + 6.0, bubble.bottom()),
                egui::pos2(bubble.left() + 4.0, bubble.bottom() + 3.0),
            ];
            painter.add(egui::Shape::convex_polygon(
                tail.to_vec(),
                colors::TEXT_PRIMARY,
                egui::Stroke::NONE,
            ));
        }
        ToolIcon::Avatar(_) => {
            painter.circle_stroke(
                rect.center_top() + egui::vec2(0.0, 10.0),
                4.0,
                stroke,
            );
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
        ToolIcon::Settings(active) => {
            let panel = egui::Rect::from_center_size(rect.center(), egui::vec2(14.0, 12.0));
            painter.rect_stroke(
                panel,
                CornerRadius::same(3),
                stroke,
                egui::StrokeKind::Middle,
            );
            let split_x = panel.right() - 4.5;
            painter.line_segment(
                [egui::pos2(split_x, panel.top()), egui::pos2(split_x, panel.bottom())],
                stroke,
            );
            if !active {
                painter.line_segment(
                    [
                        egui::pos2(panel.right() - 3.0, panel.top() + 2.0),
                        egui::pos2(panel.right() - 3.0, panel.bottom() - 2.0),
                    ],
                    egui::Stroke::new(2.2, colors::RED_ACCENT),
                );
            }
        }
    }
}