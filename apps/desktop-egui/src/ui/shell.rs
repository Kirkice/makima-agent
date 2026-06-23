use eframe::egui::{self, Frame, Margin};

use crate::app::{LoginDialogState, UiAction};
use crate::state::app_state::AppState;
use crate::theme::colors;

use super::bottom_drawer;
use super::dock::{self, AppDockState};
use super::panels::login;
use super::status_bar;
use super::top_bar;

const MENU_BAR_HEIGHT: f32 = 30.0;
const TOP_BAR_HEIGHT: f32 = 56.0;
const STATUS_BAR_HEIGHT: f32 = 32.0;
const MIN_DRAWER_HEIGHT: f32 = 160.0;
const MAX_DRAWER_HEIGHT: f32 = 360.0;

pub fn draw(
    ui: &mut egui::Ui,
    state: &mut AppState,
    login_dialog: &mut LoginDialogState,
    pending_action: &mut Option<UiAction>,
    app_dock: &mut AppDockState,
) {
    dock::sync_app_dock(app_dock, state.view_mode, state.show_context_panel, ui.available_size());

    egui::TopBottomPanel::top("makima_menu_bar")
        .exact_height(MENU_BAR_HEIGHT)
        .frame(
            Frame::NONE
                .fill(colors::SURFACE)
                .inner_margin(Margin::symmetric(12, 4))
                .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK)),
        )
        .show_inside(ui, |ui| {
            draw_menu_bar(ui, state, pending_action);
        });

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
                if login::draw(ui, state, login_dialog) {
                    *pending_action = Some(UiAction::Login);
                }
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

    egui::CentralPanel::default()
        .frame(Frame::NONE.fill(colors::BG).inner_margin(Margin::same(12)))
        .show_inside(ui, |ui| {
            dock::draw_app_dock(ui, state, app_dock, pending_action);
        });
}

fn draw_menu_bar(ui: &mut egui::Ui, state: &mut AppState, pending_action: &mut Option<UiAction>) {
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("New Chat").clicked() {
                state
                    .chat
                    .create_session(format!("Chat {}", state.chat.sessions.len() + 1));
                state.set_status("New session created".to_string());
                ui.close();
            }
            if state.is_logged_in && ui.button("Logout").clicked() {
                *pending_action = Some(UiAction::Logout);
                ui.close();
            }
        });

        ui.menu_button("Edit", |ui| {
            if ui.button("Clear Composer").clicked() {
                state.chat.composer.input.clear();
                state.set_status("Composer cleared".to_string());
                ui.close();
            }
            if ui.button("Clear Status").clicked() {
                state.clear_status();
                ui.close();
            }
        });

        ui.menu_button("View", |ui| {
            if ui
                .button(if state.show_context_panel {
                    "Hide Context Panel"
                } else {
                    "Show Context Panel"
                })
                .clicked()
            {
                state.show_context_panel = !state.show_context_panel;
                ui.close();
            }
            if ui
                .button(if state.drawer_open {
                    "Hide Bottom Drawer"
                } else {
                    "Show Bottom Drawer"
                })
                .clicked()
            {
                state.drawer_open = !state.drawer_open;
                ui.close();
            }
        });

        ui.menu_button("Help", |ui| {
            ui.label("Makima Agent");
            ui.label("Dock-based workspace");
            if let Some(path) = &state.app_config_path {
                ui.separator();
                ui.label(path);
            }
        });
    });
}
