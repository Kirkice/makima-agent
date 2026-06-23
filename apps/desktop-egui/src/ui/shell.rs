use eframe::egui::{self, Frame, Margin};

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

const TOP_BAR_HEIGHT: f32 = 56.0;
const COMPOSER_HEIGHT: f32 = 92.0;
const STATUS_BAR_HEIGHT: f32 = 32.0;
const DRAWER_HEIGHT: f32 = 220.0;
const ACTIVITY_BAR_WIDTH: f32 = 52.0;
const SIDEBAR_WIDTH: f32 = 280.0;
const INSPECTOR_WIDTH: f32 = 284.0;

pub fn draw(
    ui: &mut egui::Ui,
    state: &mut AppState,
    login_dialog: &mut LoginDialogState,
    pending_action: &mut Option<UiAction>,
    workspace_dock: &mut WorkspaceDockState,
) {
    dock::sync_workspace_dock(workspace_dock, state.view_mode);

    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

    let drawer_height = if state.drawer_open { DRAWER_HEIGHT } else { 0.0 };
    let main_height = (ui.available_height()
        - TOP_BAR_HEIGHT
        - COMPOSER_HEIGHT
        - STATUS_BAR_HEIGHT
        - drawer_height)
        .max(240.0);

    top_bar_container(ui, state);

    if !state.is_logged_in {
        Frame::NONE
            .fill(colors::BG)
            .inner_margin(Margin::same(24))
            .show(ui, |ui| {
                if login::draw(ui, state, login_dialog) {
                    *pending_action = Some(UiAction::Login);
                }
            });
        return;
    }

    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), main_height),
        egui::Layout::left_to_right(egui::Align::Min),
        |ui| {
            draw_main_columns(ui, state, pending_action, workspace_dock, main_height);
        },
    );

    composer_container(ui, state, pending_action);
    bottom_drawer::draw(ui, state);
    status_bar::draw(ui, state);
}

fn top_bar_container(ui: &mut egui::Ui, state: &mut AppState) {
    Frame::NONE
        .fill(colors::SURFACE)
        .inner_margin(Margin::symmetric(18, 10))
        .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK))
        .show(ui, |ui| {
            ui.set_height(TOP_BAR_HEIGHT);
            top_bar::draw(ui, state);
        });
}

fn draw_main_columns(
    ui: &mut egui::Ui,
    state: &mut AppState,
    pending_action: &mut Option<UiAction>,
    workspace_dock: &mut WorkspaceDockState,
    main_height: f32,
) {
    ui.allocate_ui_with_layout(
        egui::vec2(ACTIVITY_BAR_WIDTH, main_height),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            Frame::NONE
                .fill(colors::SURFACE)
                .inner_margin(Margin::symmetric(8, 14))
                .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK))
                .show(ui, |ui| {
                    ui.set_width(ACTIVITY_BAR_WIDTH);
                    activity_bar::draw(ui, state);
                });
        },
    );

    ui.add_space(8.0);

    ui.allocate_ui_with_layout(
        egui::vec2(SIDEBAR_WIDTH, main_height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            Frame::NONE
                .fill(colors::SURFACE)
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    side_nav::draw(ui, state);
                });
        },
    );

    ui.add_space(12.0);

    let workspace_width = (ui.available_width() - INSPECTOR_WIDTH - 12.0).max(420.0);
    ui.allocate_ui_with_layout(
        egui::vec2(workspace_width, main_height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            Frame::NONE
                .fill(colors::BG)
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    dock::draw_workspace(ui, state, workspace_dock, pending_action);
                });
        },
    );

    ui.add_space(12.0);

    ui.allocate_ui_with_layout(
        egui::vec2(INSPECTOR_WIDTH, main_height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            Frame::NONE
                .fill(colors::SURFACE)
                .inner_margin(Margin::same(16))
                .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK))
                .show(ui, |ui| {
                    inspector::draw(ui, state);
                });
        },
    );
}

fn composer_container(
    ui: &mut egui::Ui,
    state: &mut AppState,
    pending_action: &mut Option<UiAction>,
) {
    Frame::NONE
        .fill(colors::SURFACE)
        .inner_margin(Margin::symmetric(18, 14))
        .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK))
        .show(ui, |ui| {
            ui.set_height(COMPOSER_HEIGHT);
            if composer::draw(ui, state, pending_action) {
                *pending_action = Some(UiAction::SendMessage);
            }
        });
}
