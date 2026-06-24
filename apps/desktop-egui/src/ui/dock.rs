use eframe::egui;
use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};

use crate::app::UiAction;
use crate::state::app_state::{AppState, ViewMode};
use crate::theme::colors;

use super::activity_bar;
use super::chat::composer;
use super::panels::inspector;
use super::side_nav;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppDockTab {
    Sidebar,
    Chat,
    Avatar,
    Composer,
    Context,
}

impl AppDockTab {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Sidebar => "Sidebar",
            Self::Chat => "Chat",
            Self::Avatar => "Avatar",
            Self::Composer => "Composer",
            Self::Context => "Context",
        }
    }
}

pub type AppDockState = DockState<AppDockTab>;

const DEFAULT_SIDEBAR_WIDTH: f32 = 300.0;
const DEFAULT_CONTEXT_WIDTH: f32 = 210.0;
const DEFAULT_COMPOSER_HEIGHT: f32 = 155.0;
const MIN_SIDEBAR_PIXELS: f32 = 260.0;
const MAX_SIDEBAR_PIXELS: f32 = 340.0;
const MIN_CONTEXT_PIXELS: f32 = 180.0;
const MAX_CONTEXT_PIXELS: f32 = 240.0;
const MIN_COMPOSER_PIXELS: f32 = 140.0;
const MAX_COMPOSER_PIXELS: f32 = 220.0;

pub fn init_app_dock(
    view_mode: ViewMode,
    show_context_panel: bool,
    sidebar_width: f32,
    context_width: f32,
    available_size: egui::Vec2,
) -> AppDockState {
    let mut dock_state = DockState::new(vec![AppDockTab::Chat]);
    let surface = dock_state.main_surface_mut();

    let sidebar_pixels = normalize_pixels(sidebar_width, MIN_SIDEBAR_PIXELS, MAX_SIDEBAR_PIXELS);
    let context_pixels = normalize_pixels(context_width, MIN_CONTEXT_PIXELS, MAX_CONTEXT_PIXELS);
    let composer_pixels =
        normalize_pixels(DEFAULT_COMPOSER_HEIGHT, MIN_COMPOSER_PIXELS, MAX_COMPOSER_PIXELS);

    let total_width = available_size.x.max(1.0);
    let total_height = available_size.y.max(1.0);
    let sidebar_fraction = (sidebar_pixels / total_width).clamp(0.16, 0.32);

    let [main, _sidebar] =
        surface.split_left(NodeIndex::root(), sidebar_fraction, vec![AppDockTab::Sidebar]);

    let main_width = (total_width - sidebar_pixels).max(1.0);
    let composer_fraction = ((total_height - composer_pixels) / total_height).clamp(0.62, 0.9);

    let main = if show_context_panel {
        let context_fraction = ((main_width - context_pixels) / main_width).clamp(0.68, 0.9);
        let [chat_stack, _context] =
            surface.split_right(main, context_fraction, vec![AppDockTab::Context]);
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
    let show_context_panel = state.show_context_panel;
    let sidebar_width = state.conversations_width;
    let context_width = state.inspector_width;

    let has_chat = dock_state.iter_all_tabs().any(|(_, tab)| matches!(tab, AppDockTab::Chat));
    let has_avatar = dock_state.iter_all_tabs().any(|(_, tab)| matches!(tab, AppDockTab::Avatar));
    let has_composer = dock_state
        .iter_all_tabs()
        .any(|(_, tab)| matches!(tab, AppDockTab::Composer));
    let has_sidebar = dock_state
        .iter_all_tabs()
        .any(|(_, tab)| matches!(tab, AppDockTab::Sidebar));
    let has_context = dock_state
        .iter_all_tabs()
        .any(|(_, tab)| matches!(tab, AppDockTab::Context));

    let should_have_avatar = matches!(view_mode, ViewMode::Avatar);

    if !has_chat
        || !has_composer
        || !has_sidebar
        || has_avatar != should_have_avatar
        || has_context != show_context_panel
    {
        *dock_state = init_app_dock(
            view_mode,
            show_context_panel,
            sidebar_width,
            context_width,
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
        DEFAULT_CONTEXT_WIDTH,
        MIN_CONTEXT_PIXELS,
        MAX_CONTEXT_PIXELS,
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
            AppDockTab::Sidebar => draw_sidebar_tab(ui, self.state),
            AppDockTab::Chat => draw_chat_workspace(ui, self.state),
            AppDockTab::Avatar => crate::ui::panels::avatar::draw(ui, self.state),
            AppDockTab::Composer => {
                if composer::draw(ui, self.state, self.pending_action) {
                    *self.pending_action = Some(UiAction::SendMessage);
                }
            }
            AppDockTab::Context => inspector::draw(ui, self.state),
        }
    }

    fn clear_background(&self, _tab: &AppDockTab) -> bool {
        true
    }
}

fn draw_sidebar_tab(ui: &mut egui::Ui, state: &mut AppState) {
    let full_rect = ui.available_rect_before_wrap();
    let spacing = ui.spacing().item_spacing.x;
    let icon_width = 56.0;

    let icon_rect =
        egui::Rect::from_min_size(full_rect.min, egui::vec2(icon_width, full_rect.height()));
    let detail_min = egui::pos2(icon_rect.max.x + spacing, full_rect.min.y);
    let detail_rect = egui::Rect::from_min_max(detail_min, full_rect.max);

    ui.allocate_rect(full_rect, egui::Sense::hover());

    let mut icon_ui = ui.child_ui(
        icon_rect,
        egui::Layout::top_down(egui::Align::Min),
        None,
    );
    activity_bar::draw(&mut icon_ui, state);

    let mut detail_ui = ui.child_ui(
        detail_rect,
        egui::Layout::top_down(egui::Align::Min),
        None,
    );
    detail_ui.set_clip_rect(detail_rect);
    detail_ui.set_min_height(detail_rect.height());
    detail_ui.add_space(4.0);
    side_nav::draw(&mut detail_ui, state);
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
