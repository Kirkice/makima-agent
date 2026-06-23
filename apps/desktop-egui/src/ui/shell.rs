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
const ACTIVITY_BAR_WIDTH: f32 = 52.0;
const MIN_CONVERSATIONS_WIDTH: f32 = 220.0;
const MAX_CONVERSATIONS_WIDTH: f32 = 420.0;
const MIN_INSPECTOR_WIDTH: f32 = 220.0;
const MAX_INSPECTOR_WIDTH: f32 = 380.0;
const MIN_DRAWER_HEIGHT: f32 = 160.0;
const MAX_DRAWER_HEIGHT: f32 = 360.0;

pub fn draw(
    ui: &mut egui::Ui,
    state: &mut AppState,
    login_dialog: &mut LoginDialogState,
    pending_action: &mut Option<UiAction>,
    workspace_dock: &mut WorkspaceDockState,
) {
    dock::sync_workspace_dock(workspace_dock, state.view_mode);

    egui::TopBottomPanel::top("makima_top_bar")
        .exact_height(TOP_BAR_HEIGHT)
        .frame(
            Frame::NONE
                .fill(colors::SURFACE)
                .inner_margin(Margin::symmetric(18, 10))
                .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK)),
        )
        .show_inside(ui, |ui| {
            top_bar::draw(ui, state);
        });

    if !state.is_logged_in {
        Frame::NONE
            .fill(colors::BG)
            .inner_margin(Margin::same(24))
            .show(ui, |ui| {
                let available = ui.available_size();
                ui.allocate_ui_with_layout(
                    available,
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        if login::draw(ui, state, login_dialog) {
                            *pending_action = Some(UiAction::Login);
                        }
                    },
                );
            });
        return;
    }

    egui::TopBottomPanel::bottom("makima_status_bar")
        .exact_height(STATUS_BAR_HEIGHT)
        .frame(Frame::NONE)
        .show_inside(ui, |ui| {
            status_bar::draw(ui, state);
        });

    if state.drawer_open {
        egui::TopBottomPanel::bottom("makima_bottom_drawer")
            .resizable(true)
            .default_height(state.drawer_height)
            .min_height(MIN_DRAWER_HEIGHT)
            .max_height(MAX_DRAWER_HEIGHT)
            .frame(Frame::NONE)
            .show_inside(ui, |ui| {
                state.drawer_height = ui.available_height().clamp(MIN_DRAWER_HEIGHT, MAX_DRAWER_HEIGHT);
                bottom_drawer::draw(ui, state);
            });
    }

    egui::TopBottomPanel::bottom("makima_composer")
        .exact_height(COMPOSER_HEIGHT)
        .frame(
            Frame::NONE
                .fill(colors::SURFACE)
                .inner_margin(Margin::symmetric(18, 14))
                .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK)),
        )
        .show_inside(ui, |ui| {
            if composer::draw(ui, state, pending_action) {
                *pending_action = Some(UiAction::SendMessage);
            }
        });

    egui::SidePanel::left("makima_activity_bar")
        .exact_width(ACTIVITY_BAR_WIDTH + 16.0)
        .frame(
            Frame::NONE
                .fill(colors::SURFACE)
                .inner_margin(Margin::symmetric(8, 14))
                .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK)),
        )
        .show_inside(ui, |ui| {
            ui.set_width(ACTIVITY_BAR_WIDTH);
            activity_bar::draw(ui, state);
        });

    egui::SidePanel::left("makima_conversations")
        .resizable(true)
        .default_width(state.conversations_width)
        .min_width(MIN_CONVERSATIONS_WIDTH)
        .max_width(MAX_CONVERSATIONS_WIDTH)
        .frame(
            Frame::NONE
                .fill(colors::SURFACE)
                .inner_margin(Margin::same(16)),
        )
        .show_inside(ui, |ui| {
            state.conversations_width =
                ui.available_width().clamp(MIN_CONVERSATIONS_WIDTH, MAX_CONVERSATIONS_WIDTH);
            side_nav::draw(ui, state);
        });

    egui::SidePanel::right("makima_inspector")
        .resizable(true)
        .default_width(state.inspector_width)
        .min_width(MIN_INSPECTOR_WIDTH)
        .max_width(MAX_INSPECTOR_WIDTH)
        .frame(
            Frame::NONE
                .fill(colors::SURFACE)
                .inner_margin(Margin::same(16))
                .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK)),
        )
        .show_inside(ui, |ui| {
            state.inspector_width =
                ui.available_width().clamp(MIN_INSPECTOR_WIDTH, MAX_INSPECTOR_WIDTH);
            inspector::draw(ui, state);
        });

    egui::CentralPanel::default()
        .frame(Frame::NONE.fill(colors::BG).inner_margin(Margin::same(16)))
        .show_inside(ui, |ui| {
            dock::draw_workspace(ui, state, workspace_dock, pending_action);
        });
}
