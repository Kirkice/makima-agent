//! Activity Bar - 52px 图标导航栏
//!
//! 提供主要功能区的快速访问入口，使用产品化分组：
//! - Sessions（会话）
//! - Resources（资源）
//! - Agent（智能体）
//! - Integrations（集成）

use eframe::egui::{self, CornerRadius, Frame, Margin, Sense, Vec2};
use crate::state::app_state::{AppState, ActivitySection};
use crate::theme::colors;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.vertical(|ui| {
        ui.add_space(8.0);

        // Sessions - 会话
        if draw_icon_button(ui, "💬", "Sessions", state.activity_section == ActivitySection::Sessions) {
            state.activity_section = ActivitySection::Sessions;
        }

        ui.add_space(4.0);

        // Resources - 资源（Knowledge, Memory）
        if draw_icon_button(ui, "📚", "Resources", state.activity_section == ActivitySection::Resources) {
            state.activity_section = ActivitySection::Resources;
        }

        ui.add_space(4.0);

        // Agent - 智能体（Modes, Persona）
        if draw_icon_button(ui, "🤖", "Agent", state.activity_section == ActivitySection::Agent) {
            state.activity_section = ActivitySection::Agent;
        }

        ui.add_space(4.0);

        // Integrations - 集成（MCP, Voice）
        if draw_icon_button(ui, "🔌", "Integrations", state.activity_section == ActivitySection::Integrations) {
            state.activity_section = ActivitySection::Integrations;
        }
    });
}

fn draw_icon_button(
    ui: &mut egui::Ui,
    icon: &str,
    tooltip: &str,
    active: bool,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::click());

    let bg = if active {
        colors::ELEVATED
    } else if response.hovered() {
        colors::SURFACE
    } else {
        colors::TRANSPARENT
    };

    let fg = if active {
        colors::ICON_ACTIVE
    } else {
        colors::ICON_DEFAULT
    };

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(10), bg);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(18.0),
        fg,
    );

    response.on_hover_text(tooltip).clicked()
}