//! Main Workspace Dock — only Chat and Avatar tabs
//!
//! The workspace dock is intentionally minimal: only two tabs
//! that represent the primary work surfaces. All other panels
//! live in fixed sidebars or the bottom drawer.

use eframe::egui;
use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};
use crate::app::UiAction;
use crate::state::app_state::{AppState, ViewMode};
use crate::theme::colors;

/// Workspace tabs — strictly limited to Chat and Avatar
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkspaceTab {
    Chat,
    Avatar,
}

impl WorkspaceTab {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Chat => "💬 Chat",
            Self::Avatar => "🧑 Avatar",
        }
    }
}

/// Type alias for workspace dock state
pub type WorkspaceDockState = DockState<WorkspaceTab>;

/// Initialize workspace dock based on view mode
pub fn init_workspace_dock(view_mode: ViewMode) -> WorkspaceDockState {
    match view_mode {
        ViewMode::Chat => {
            // Chat Focus: only Chat tab
            DockState::new(vec![WorkspaceTab::Chat])
        }
        ViewMode::Avatar => {
            // Avatar Focus: Chat + Avatar side by side
            let mut ds = DockState::new(vec![WorkspaceTab::Chat]);
            let surface = ds.main_surface_mut();
            surface.split_right(NodeIndex::root(), 0.5, vec![WorkspaceTab::Avatar]);
            ds
        }
    }
}

struct WorkspaceTabViewer<'a> {
    state: &'a mut AppState,
    pending_action: &'a mut Option<UiAction>,
}

impl TabViewer for WorkspaceTabViewer<'_> {
    type Tab = WorkspaceTab;

    fn title(&mut self, tab: &mut WorkspaceTab) -> egui::WidgetText {
        egui::RichText::new(tab.title()).size(13.0).into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut WorkspaceTab) {
        match tab {
            WorkspaceTab::Chat => {
                draw_chat_workspace(ui, self.state, self.pending_action);
            }
            WorkspaceTab::Avatar => {
                crate::ui::panels::avatar::draw(ui, self.state);
            }
        }
    }

    fn clear_background(&self, _tab: &WorkspaceTab) -> bool {
        true
    }
}

/// Draw the Chat workspace content (transcript only — composer is in shell)
fn draw_chat_workspace(ui: &mut egui::Ui, state: &mut AppState, _pending_action: &mut Option<UiAction>) {
    // Show inline task timeline hint if task is active
    if let Some(task) = &state.task.active_task {
        draw_inline_task_hint(ui, task);
    }

    // Transcript
    if let Some(session) = state.chat.active_session_mut() {
        crate::ui::chat::transcript::draw(ui, session, &state.task.active_task);
    } else {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            ui.colored_label(colors::TEXT_MUTED, "Select or create a conversation.");
        });
    }
}

/// Lightweight task timeline hint embedded in chat flow
fn draw_inline_task_hint(ui: &mut egui::Ui, task: &crate::state::task_state::TaskExecutionState) {
    use crate::state::task_state::TaskStatus;

    let (icon, label, color) = match task.status {
        TaskStatus::Running => ("⏳", "Task running...", colors::INFO),
        TaskStatus::Idle => return, // Don't show if idle
        _ => ("✓", "Task complete", colors::SUCCESS),
    };

    egui::Frame::NONE
        .fill(colors::ELEVATED)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(color, icon);
                ui.colored_label(colors::TEXT_SECONDARY, label);
                if task.timeline.len() > 0 {
                    ui.colored_label(colors::TEXT_MUTED, format!("({} steps)", task.timeline.len()));
                }
            });
        });
    ui.add_space(8.0);
}

/// Draw the workspace dock area
pub fn draw_workspace(
    ui: &mut egui::Ui,
    state: &mut AppState,
    dock_state: &mut WorkspaceDockState,
    pending_action: &mut Option<UiAction>,
) {
    let style = Style::from_egui(ui.style().as_ref());

    DockArea::new(dock_state)
        .style(style)
        .show_leaf_close_all_buttons(false)
        .show_leaf_collapse_buttons(false)
        .show_add_buttons(false)
        .show_inside(ui, &mut WorkspaceTabViewer { state, pending_action });
}
