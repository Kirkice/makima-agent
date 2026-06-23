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

const DEFAULT_COMPOSER_HEIGHT: f32 = 180.0;

pub fn init_app_dock(
    view_mode: ViewMode,
    show_context_panel: bool,
    sidebar_width: f32,
    context_width: f32,
    available_size: egui::Vec2,
) -> AppDockState {
    let mut dock_state = DockState::new(vec![AppDockTab::Chat]);
    let surface = dock_state.main_surface_mut();

    let sidebar_fraction = retained_fraction(available_size.x, sidebar_width);
    let context_fraction = retained_fraction(available_size.x, context_width);
    let composer_fraction = retained_fraction(available_size.y, DEFAULT_COMPOSER_HEIGHT);

    let [main, _sidebar] =
        surface.split_left(NodeIndex::root(), sidebar_fraction, vec![AppDockTab::Sidebar]);
    let [main, _composer] =
        surface.split_below(main, composer_fraction, vec![AppDockTab::Composer]);

    if matches!(view_mode, ViewMode::Avatar) {
        let [_chat, _avatar] = surface.split_right(main, 0.58, vec![AppDockTab::Avatar]);
    }

    if show_context_panel {
        let _ = surface.split_right(NodeIndex::root(), context_fraction, vec![AppDockTab::Context]);
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

fn retained_fraction(total: f32, panel_pixels: f32) -> f32 {
    if total <= panel_pixels {
        0.5
    } else {
        ((total - panel_pixels) / total).clamp(0.2, 0.92)
    }
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
    ui.horizontal(|ui| {
        // Activity icon bar — fixed 56px width
        let icon_size = egui::vec2(56.0, ui.available_height());
        let (icon_rect, _) = ui.allocate_exact_size(icon_size, egui::Sense::hover());
        let mut child_ui = ui.child_ui(
            icon_rect,
            egui::Layout::top_down(egui::Align::Min),
            None,
        );
        activity_bar::draw(&mut child_ui, state);
        // Detail panel — takes remaining width
        ui.vertical(|ui| {
            ui.add_space(4.0);
            side_nav::draw(ui, state);
        });
    });
}

fn draw_chat_workspace(ui: &mut egui::Ui, state: &mut AppState) {
    if let Some(task) = &state.task.active_task {
        draw_inline_task_hint(ui, task);
    }

    if let Some(session) = state.chat.active_session_mut() {
        crate::ui::chat::transcript::draw(ui, session, &state.task.active_task);
    } else {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
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
                ui.colored_label(color, "•");
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
