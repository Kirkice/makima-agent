//! Shell — 应用固定骨架布局
//!
//! 采用"固定骨架 + 局部 dock"的架构：
//! - Top Bar (56px)
//! - Activity Bar (52px) | Conversations (280px) | Main Workspace (dock) | Inspector (300px)
//! - Composer (72px)
//! - Bottom Drawer (按需浮现)
//! - Status Bar

use eframe::egui::{self, CornerRadius, Frame, Margin};

use crate::app::{LoginDialogState, UiAction};
use crate::state::app_state::AppState;
use crate::theme::colors;

use super::activity_bar;
use super::bottom_drawer;
use super::chat::composer;
use super::dock::{self, WorkspaceDockState};
use super::panels::inspector;
use super::panels::login;
use super::side_nav;
use super::status_bar;
use super::top_bar;

pub fn draw(
    ui: &mut egui::Ui,
    state: &mut AppState,
    login_dialog: &mut LoginDialogState,
    pending_action: &mut Option<UiAction>,
    workspace_dock: &mut WorkspaceDockState,
) {
    // ═══════════════════════════════════════════════════════════════════
    // 1. Top Bar (56px) — 品牌、session 标题、状态、workspace switch
    // ═══════════════════════════════════════════════════════════════════
    let mut view_mode_switch = None;
    Frame::NONE
        .fill(colors::SURFACE)
        .inner_margin(Margin::symmetric(16, 14))
        .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK))
        .show(ui, |ui| {
            view_mode_switch = top_bar::draw(ui, state);
        });
    
    // 处理 view_mode 切换：重建 workspace_dock
    if let Some(new_mode) = view_mode_switch {
        state.view_mode = new_mode;
        *workspace_dock = crate::ui::dock::init_workspace_dock(new_mode);
    }

    // 登录状态判断
    if !state.is_logged_in {
        if login::draw(ui, state, login_dialog) {
            *pending_action = Some(UiAction::Login);
        }
        return;
    }

    // ═══════════════════════════════════════════════════════════════════
    // 2. 主区域水平布局
    // ═══════════════════════════════════════════════════════════════════
    ui.horizontal(|ui| {
        // ── Activity Bar (52px) ──────────────────────────────────────
        Frame::NONE
            .fill(colors::SURFACE)
            .inner_margin(Margin::symmetric(8, 12))
            .show(ui, |ui| {
                ui.set_width(52.0);
                activity_bar::draw(ui, state);
            });

        // 分隔线
        ui.add(egui::Separator::default().vertical().spacing(0.0));

        // ── Conversations Sidebar (280px) ────────────────────────────
        Frame::NONE
            .fill(colors::BG)
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(280.0);
                // Activity Bar 驱动侧边栏内容切换
                match state.activity_section {
                    crate::state::app_state::ActivitySection::Sessions => {
                        side_nav::draw(ui, state);
                    }
                    crate::state::app_state::ActivitySection::Resources => {
                        ui.colored_label(colors::TEXT_PRIMARY, "Resources");
                        ui.separator();
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            crate::ui::panels::memory::draw(ui, state);
                            ui.add_space(12.0);
                            crate::ui::panels::knowledge::draw(ui, state);
                        });
                    }
                    crate::state::app_state::ActivitySection::Agent => {
                        ui.colored_label(colors::TEXT_PRIMARY, "Agent");
                        ui.separator();
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            crate::ui::panels::modes::draw(ui, state);
                            ui.add_space(12.0);
                            crate::ui::panels::persona::draw(ui, state);
                        });
                    }
                    crate::state::app_state::ActivitySection::Integrations => {
                        ui.colored_label(colors::TEXT_PRIMARY, "Integrations");
                        ui.separator();
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            crate::ui::panels::mcp::draw(ui, state);
                            ui.add_space(12.0);
                            crate::ui::panels::voice::draw(ui, state);
                        });
                    }
                }
            });

        // 分隔线
        ui.add(egui::Separator::default().vertical().spacing(0.0));

        // ── Main Workspace (dock: Chat / Avatar) ─────────────────────
        let available_width = (ui.available_width() - 300.0).max(200.0); // 预留 Inspector，最小 200px
        Frame::NONE
            .fill(colors::BG)
            .show(ui, |ui| {
                ui.set_width(available_width);
                dock::draw_workspace(ui, state, workspace_dock, pending_action);
            });

        // 分隔线
        ui.add(egui::Separator::default().vertical().spacing(0.0));

        // ── Inspector Sidebar (300px) ────────────────────────────────
        Frame::NONE
            .fill(colors::SURFACE)
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(300.0);
                inspector::draw(ui, state);
            });
    });

    // ═══════════════════════════════════════════════════════════════════
    // 3. Composer (72px) — 固定底部，独立于 dock
    // ═══════════════════════════════════════════════════════════════════
    Frame::NONE
        .fill(colors::SURFACE)
        .inner_margin(Margin::symmetric(16, 12))
        .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK))
        .show(ui, |ui| {
            if composer::draw(ui, state, pending_action) {
                *pending_action = Some(UiAction::SendMessage);
            }
        });

    // ═══════════════════════════════════════════════════════════════════
    // 4. Bottom Drawer — 按需浮现（TaskTimeline, VoiceCall, Audit 等）
    // ═══════════════════════════════════════════════════════════════════
    bottom_drawer::draw(ui, state);

    // ═══════════════════════════════════════════════════════════════════
    // 5. Status Bar (24px) — 底部状态栏
    // ═══════════════════════════════════════════════════════════════════
    status_bar::draw(ui, state);
}