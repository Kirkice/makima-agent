//! Top Bar - 顶部栏 (56px)
//!
//! 显示品牌标识、当前会话标题、workspace 切换按钮、连接状态

use eframe::egui::{self, Frame, Margin, Vec2};
use crate::state::app_state::{AppState, ViewMode};
use crate::theme::colors;

/// 绘制顶部栏，返回需要切换的 ViewMode（如果用户点击了切换按钮）
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) -> Option<ViewMode> {
    let mut switch_to = None;
    
    ui.horizontal(|ui| {
        // Brand / Title
        ui.colored_label(colors::ICON_ACTIVE, "◆");
        ui.add_space(8.0);
        ui.colored_label(colors::TEXT_PRIMARY, "Makima");

        // Current session title
        if let Some(session) = state.chat.active_session() {
            ui.add_space(16.0);
            ui.colored_label(colors::TEXT_SECONDARY, &session.title);
        }

        // Spacer
        ui.add_space(ui.available_width() - 200.0);

        // Workspace switch (Chat / Avatar)
        if let Some(mode) = draw_workspace_switch(ui, state) {
            switch_to = Some(mode);
        }

        ui.add_space(16.0);

        // Connection status indicator
        let (color, text) = if state.settings.health.sse_connected {
            (colors::SUCCESS, "Online")
        } else {
            (colors::TEXT_MUTED, "Offline")
        };
        ui.colored_label(color, format!("● {}", text));
    });
    
    switch_to
}

fn draw_workspace_switch(ui: &mut egui::Ui, state: &mut AppState) -> Option<ViewMode> {
    let mut result = None;
    
    ui.horizontal(|ui| {
        // Chat button
        let chat_style = if state.view_mode == ViewMode::Chat {
            egui::Button::new("💬 Chat").fill(colors::ELEVATED).small()
        } else {
            egui::Button::new("💬 Chat").fill(colors::TRANSPARENT).small()
        };
        if ui.add(chat_style).clicked() && state.view_mode != ViewMode::Chat {
            result = Some(ViewMode::Chat);
        }

        // Avatar button
        let avatar_style = if state.view_mode == ViewMode::Avatar {
            egui::Button::new("🧑 Avatar").fill(colors::ELEVATED).small()
        } else {
            egui::Button::new("🧑 Avatar").fill(colors::TRANSPARENT).small()
        };
        if ui.add(avatar_style).clicked() && state.view_mode != ViewMode::Avatar {
            result = Some(ViewMode::Avatar);
        }
    });
    
    result
}
