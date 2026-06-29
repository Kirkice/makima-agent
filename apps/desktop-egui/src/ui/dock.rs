use eframe::egui;
use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};

use crate::app::UiAction;
use crate::state::app_state::{AppState, ViewMode};
use crate::theme::colors;

use super::chat::composer;
use super::panels::settings;
use super::recent_chat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppDockTab {
    RecentChat,
    Chat,
    Avatar,
    Composer,
    Settings,
}

impl AppDockTab {
    pub fn title(&self) -> &'static str {
        match self {
            Self::RecentChat => "Recent",
            Self::Chat => "Chat",
            Self::Avatar => "Avatar",
            Self::Composer => "Composer",
            Self::Settings => "Settings",
        }
    }
}

pub type AppDockState = DockState<AppDockTab>;

const DEFAULT_SIDEBAR_WIDTH: f32 = 300.0;
const DEFAULT_SETTINGS_WIDTH: f32 = 420.0;
const DEFAULT_COMPOSER_HEIGHT: f32 = 155.0;
const MIN_SIDEBAR_PIXELS: f32 = 260.0;
const MAX_SIDEBAR_PIXELS: f32 = 340.0;
const MIN_SETTINGS_PIXELS: f32 = 320.0;
const MAX_SETTINGS_PIXELS: f32 = 560.0;
const MIN_COMPOSER_PIXELS: f32 = 140.0;
const MAX_COMPOSER_PIXELS: f32 = 220.0;

pub fn init_app_dock(
    view_mode: ViewMode,
    show_settings_panel: bool,
    sidebar_width: f32,
    settings_width: f32,
    available_size: egui::Vec2,
) -> AppDockState {
    let mut dock_state = DockState::new(vec![AppDockTab::Chat]);
    let surface = dock_state.main_surface_mut();

    let sidebar_pixels = normalize_pixels(sidebar_width, MIN_SIDEBAR_PIXELS, MAX_SIDEBAR_PIXELS);
    let settings_pixels = normalize_pixels(settings_width, MIN_SETTINGS_PIXELS, MAX_SETTINGS_PIXELS);
    let composer_pixels =
        normalize_pixels(DEFAULT_COMPOSER_HEIGHT, MIN_COMPOSER_PIXELS, MAX_COMPOSER_PIXELS);

    let total_width = available_size.x.max(1.0);
    let total_height = available_size.y.max(1.0);
    let sidebar_fraction = (sidebar_pixels / total_width).clamp(0.16, 0.32);

    let [main, _sidebar] =
        surface.split_left(NodeIndex::root(), sidebar_fraction, vec![AppDockTab::RecentChat]);

    let main_width = (total_width - sidebar_pixels).max(1.0);
    let composer_fraction = ((total_height - composer_pixels) / total_height).clamp(0.62, 0.9);

    let main = if show_settings_panel {
        let settings_fraction = ((main_width - settings_pixels) / main_width).clamp(0.62, 0.85);
        let [chat_stack, _settings] =
            surface.split_right(main, settings_fraction, vec![AppDockTab::Settings]);
        chat_stack
    } else {
        main
    };

    let [main, _composer] =
        surface.split_below(main, composer_fraction, vec![AppDockTab::Composer]);

    if matches!(view_mode, ViewMode::Avatar) {
        let [_chat, _avatar] = surface.split_right(main, 0.58, vec![AppDockTab::Avatar]);
    }

    dock_state
}

pub fn sync_app_dock(
    dock_state: &mut AppDockState,
    state: &AppState,
    available_size: egui::Vec2,
) {
    let view_mode = state.view_mode;
    let show_settings_panel = state.show_settings_panel;
    let sidebar_width = state.conversations_width;
    let settings_width = state.inspector_width;

    let has_chat = dock_state.iter_all_tabs().any(|(_, tab)| matches!(tab, AppDockTab::Chat));
    let has_avatar = dock_state.iter_all_tabs().any(|(_, tab)| matches!(tab, AppDockTab::Avatar));
    let has_composer = dock_state
        .iter_all_tabs()
        .any(|(_, tab)| matches!(tab, AppDockTab::Composer));
    let has_sidebar = dock_state
        .iter_all_tabs()
        .any(|(_, tab)| matches!(tab, AppDockTab::RecentChat));
    let has_settings = dock_state
        .iter_all_tabs()
        .any(|(_, tab)| matches!(tab, AppDockTab::Settings));

    let should_have_avatar = matches!(view_mode, ViewMode::Avatar);

    if !has_chat
        || !has_composer
        || !has_sidebar
        || has_avatar != should_have_avatar
        || has_settings != show_settings_panel
    {
        *dock_state = init_app_dock(
            view_mode,
            show_settings_panel,
            sidebar_width,
            settings_width,
            available_size,
        );
    }
}

pub fn normalize_layout(state: &mut AppState) {
    state.conversations_width = normalized_panel_pixels(
        state.conversations_width,
        DEFAULT_SIDEBAR_WIDTH,
        MIN_SIDEBAR_PIXELS,
        MAX_SIDEBAR_PIXELS,
    );
    state.inspector_width = normalized_panel_pixels(
        state.inspector_width,
        DEFAULT_SETTINGS_WIDTH,
        MIN_SETTINGS_PIXELS,
        MAX_SETTINGS_PIXELS,
    );
    state.drawer_height = normalized_panel_pixels(
        state.drawer_height,
        DEFAULT_COMPOSER_HEIGHT,
        MIN_COMPOSER_PIXELS,
        MAX_COMPOSER_PIXELS,
    );
}

fn normalized_panel_pixels(
    current: f32,
    default_pixels: f32,
    min_pixels: f32,
    max_pixels: f32,
) -> f32 {
    if current <= 0.0 {
        default_pixels
    } else {
        current.clamp(min_pixels, max_pixels)
    }
}

fn normalize_pixels(current: f32, min_pixels: f32, max_pixels: f32) -> f32 {
    current.clamp(min_pixels, max_pixels)
}

struct AppTabViewer<'a> {
    state: &'a mut AppState,
    pending_action: &'a mut Option<UiAction>,
}

impl TabViewer for AppTabViewer<'_> {
    type Tab = AppDockTab;

    fn title(&mut self, tab: &mut AppDockTab) -> egui::WidgetText {
        egui::RichText::new(tab.title()).size(13.0).into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut AppDockTab) {
        match tab {
            AppDockTab::RecentChat => draw_sidebar_tab(ui, self.state),
            AppDockTab::Chat => draw_chat_workspace(ui, self.state),
            AppDockTab::Avatar => crate::ui::panels::avatar_impl::draw(ui, self.state),
            AppDockTab::Composer => {
                if composer::draw(ui, self.state, self.pending_action) {
                    *self.pending_action = Some(UiAction::SendMessage);
                }
            }
            AppDockTab::Settings => settings::draw(ui, self.state),
        }
    }

    fn clear_background(&self, _tab: &AppDockTab) -> bool {
        true
    }
}

fn draw_sidebar_tab(ui: &mut egui::Ui, state: &mut AppState) {
    recent_chat::draw(ui, state);
}

fn draw_chat_workspace(ui: &mut egui::Ui, state: &mut AppState) {
    if let Some(task) = &state.task.active_task {
        draw_inline_task_hint(ui, task);
    }

    if let Some(session) = state.chat.active_session_mut() {
        crate::ui::chat::transcript::draw(ui, session, &state.task.active_task);
    } else {
        ui.centered_and_justified(|ui| {
            ui.colored_label(colors::TEXT_MUTED, "Select or create a conversation.");
        });
    }
}

fn draw_inline_task_hint(ui: &mut egui::Ui, task: &crate::state::task_state::TaskExecutionState) {
    use crate::state::task_state::TaskStatus;

    let (label, color) = match task.status {
        TaskStatus::Running => ("Task running...", colors::INFO),
        TaskStatus::Idle => return,
        _ => ("Task complete", colors::SUCCESS),
    };

    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(color, "...");
                ui.colored_label(colors::TEXT_SECONDARY, label);
                if !task.timeline.is_empty() {
                    ui.colored_label(colors::TEXT_MUTED, format!("({} steps)", task.timeline.len()));
                }
            });
        });
    ui.add_space(8.0);
}

pub fn draw_app_dock(
    ui: &mut egui::Ui,
    state: &mut AppState,
    dock_state: &mut AppDockState,
    pending_action: &mut Option<UiAction>,
) {
    let style = Style::from_egui(ui.style().as_ref());

    DockArea::new(dock_state)
        .style(style)
        .show_leaf_close_all_buttons(false)
        .show_leaf_collapse_buttons(false)
        .show_add_buttons(false)
        .show_inside(
            ui,
            &mut AppTabViewer {
                state,
                pending_action,
            },
        );
}