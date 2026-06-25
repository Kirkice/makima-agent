use eframe::egui::{self, Frame, Margin};

use crate::app::{LoginDialogState, UiAction};
use crate::state::app_state::{AppState, ViewMode};
use crate::theme::colors;

use super::bottom_drawer;
use super::dock::{self, AppDockState};
use super::panels::{audit, knowledge, login, mcp, memory, model_config, modes, persona, diagnostics, voice};
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
    dock::sync_app_dock(app_dock, state, ui.available_size());

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

    // Floating windows for orphaned panels
    draw_floating_windows(ui.ctx(), state);
}

fn draw_floating_windows(ctx: &egui::Context, state: &mut AppState) {
    if state.show_window_model_config {
        let mut open = true;
        egui::Window::new("Model Configuration")
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .default_height(500.0)
            .min_width(350.0)
            .max_width(700.0)
            .show(ctx, |ui| {
                model_config::draw(ui, state);
            });
        if !open { state.show_window_model_config = false; }
    }
    if state.show_window_mcp {
        let mut open = true;
        egui::Window::new("MCP Servers")
            .open(&mut open)
            .resizable(true)
            .default_width(460.0)
            .default_height(420.0)
            .show(ctx, |ui| {
                mcp::draw(ui, state);
            });
        if !open { state.show_window_mcp = false; }
    }
    if state.show_window_audit {
        let mut open = true;
        egui::Window::new("Audit Log")
            .open(&mut open)
            .resizable(true)
            .default_width(640.0)
            .default_height(420.0)
            .show(ctx, |ui| {
                audit::draw(ui, state);
            });
        if !open { state.show_window_audit = false; }
    }
    if state.show_window_persona {
        let mut open = true;
        egui::Window::new("Persona")
            .open(&mut open)
            .resizable(true)
            .default_width(460.0)
            .default_height(420.0)
            .show(ctx, |ui| {
                persona::draw(ui, state);
            });
        if !open { state.show_window_persona = false; }
    }
    if state.show_window_diagnostics {
        let mut open = true;
        egui::Window::new("Diagnostics")
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .default_height(380.0)
            .show(ctx, |ui| {
                diagnostics::draw(ui, state);
            });
        if !open { state.show_window_diagnostics = false; }
    }
    if state.show_window_modes {
        let mut open = true;
        egui::Window::new("Modes")
            .open(&mut open)
            .resizable(true)
            .default_width(520.0)
            .default_height(440.0)
            .show(ctx, |ui| {
                modes::draw(ui, state);
            });
        if !open { state.show_window_modes = false; }
    }
    if state.show_window_memory {
        let mut open = true;
        egui::Window::new("Memory")
            .open(&mut open)
            .resizable(true)
            .default_width(520.0)
            .default_height(440.0)
            .show(ctx, |ui| {
                memory::draw(ui, state);
            });
        if !open { state.show_window_memory = false; }
    }
    if state.show_window_knowledge {
        let mut open = true;
        egui::Window::new("Knowledge")
            .open(&mut open)
            .resizable(true)
            .default_width(520.0)
            .default_height(440.0)
            .show(ctx, |ui| {
                knowledge::draw(ui, state);
            });
        if !open { state.show_window_knowledge = false; }
    }
    if state.show_window_voice {
        let mut open = true;
        egui::Window::new("Voice")
            .open(&mut open)
            .resizable(true)
            .default_width(460.0)
            .default_height(420.0)
            .show(ctx, |ui| {
                voice::draw(ui, state);
            });
        if !open { state.show_window_voice = false; }
    }
}

fn draw_menu_bar(ui: &mut egui::Ui, state: &mut AppState, pending_action: &mut Option<UiAction>) {
    ui.horizontal(|ui| {
        // 左侧：菜单按钮
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

        // 右侧：工具按钮（仅在登录后显示）
        if state.is_logged_in {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // 右侧栏显示/隐藏按钮
                let context_icon = if state.show_context_panel { "⊞" } else { "⊟" };
                if ui
                    .button(context_icon)
                    .on_hover_text("Toggle Context Panel")
                    .clicked()
                {
                    state.show_context_panel = !state.show_context_panel;
                }

                // 模型显示按钮
                if ui.button("⚙").on_hover_text("Model Settings").clicked() {
                    state.show_window_model_config = true;
                    state.api_commands.push(crate::state::app_state::ApiCommand::FetchModelProfiles);
                }

                // 聊天按钮
                let chat_active = state.view_mode == ViewMode::Chat;
                let chat_btn = egui::Button::new("💬")
                    .fill(if chat_active { colors::RED_ACCENT } else { colors::TRANSPARENT });
                if ui
                    .add(chat_btn)
                    .on_hover_text("Chat View")
                    .clicked()
                {
                    state.view_mode = ViewMode::Chat;
                }

                // Avatar 按钮
                let avatar_active = state.view_mode == ViewMode::Avatar;
                let avatar_btn = egui::Button::new("👤")
                    .fill(if avatar_active { colors::RED_ACCENT } else { colors::TRANSPARENT });
                if ui
                    .add(avatar_btn)
                    .on_hover_text("Avatar View")
                    .clicked()
                {
                    state.view_mode = ViewMode::Avatar;
                }
            });
        }
    });
}
