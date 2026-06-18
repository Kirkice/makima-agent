use crate::app::{LoginDialogState, UiAction};
use crate::state::app_state::{AppState, PanelKind};
use crate::theme::colors;

use super::chat::{composer, transcript};
use super::panels::{audit, diagnostics, inspector, knowledge, login, mcp, memory, model_config, modes, persona, voice};
use super::side_nav;
use super::top_bar;

pub fn draw(
    ui: &mut egui::Ui,
    state: &mut AppState,
    login_dialog: &mut LoginDialogState,
    pending_action: &mut Option<UiAction>,
) {
    top_bar::draw(ui, state);

    egui::Frame::none()
        .fill(colors::GRAPHITE_BG)
        .inner_margin(egui::Margin::symmetric(0.0, 0.0))
        .show(ui, |ui| {
            if state.show_login || !state.is_logged_in {
                login::draw(ui, state, login_dialog);
                if login_dialog.loading {
                    *pending_action = Some(UiAction::Login);
                }
                return;
            }

            // If a full panel is selected, show it instead of 3-column layout
            if let Some(panel) = state.show_panel {
                draw_panel(ui, state, panel);
                return;
            }

            // Main 3-column layout
            let available = ui.available_size();
            let left_width = 220.0_f32.min(available.x * 0.2);
            let inspector_width = if state.chat.show_inspector { 280.0_f32.min(available.x * 0.25) } else { 0.0 };
            let center_width = available.x - left_width - inspector_width;

            ui.horizontal(|ui| {
                // Left Rail
                ui.allocate_ui_with_layout(
                    egui::vec2(left_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| { side_nav::draw(ui, state); },
                );

                // Center
                ui.allocate_ui_with_layout(
                    egui::vec2(center_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::Frame::none().fill(colors::GRAPHITE_BG).inner_margin(egui::Margin::symmetric(0.0, 0.0))
                            .show(ui, |ui| {
                                egui::TopBottomPanel::bottom("transcript_area").resizable(false).min_height(100.0)
                                    .show_inside(ui, |ui| {
                                        if let Some(session) = state.chat.active_session_mut() {
                                            transcript::draw(ui, session, &state.task.active_task);
                                        } else {
                                            ui.vertical_centered(|ui| {
                                                ui.add_space(ui.available_height() * 0.3);
                                                ui.colored_label(colors::TEXT_MUTED, "Select or create a conversation.");
                                            });
                                        }
                                    });
                                egui::TopBottomPanel::bottom("composer_area").resizable(false).min_height(80.0)
                                    .show_inside(ui, |ui| {
                                        if composer::draw(ui, state) {
                                            *pending_action = Some(UiAction::SendMessage);
                                        }
                                    });
                            });
                    },
                );

                // Right Inspector
                if state.chat.show_inspector && inspector_width > 0.0 {
                    ui.allocate_ui_with_layout(
                        egui::vec2(inspector_width, ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| { inspector::draw(ui, state); },
                    );
                }
            });
        });

    // Overlay panels
    if state.show_diagnostics {
        egui::Window::new("Diagnostics").id(egui::Id::new("diag")).default_height(ui.available_height() * 0.4)
            .resizable(true).collapsible(true).show(ui.ctx(), |ui| diagnostics::draw(ui, state));
    }

    login::draw_status_bar(ui, state);
}

fn draw_panel(ui: &mut egui::Ui, state: &mut AppState, panel: PanelKind) {
    egui::Frame::none().fill(colors::GRAPHITE_SURFACE)
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("← Back").clicked() { state.show_panel = None; }
                ui.add_space(8.0);
                ui.colored_label(colors::TEXT_PRIMARY, panel_label(panel));
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                match panel {
                    PanelKind::Modes => modes::draw(ui, state),
                    PanelKind::Memory => memory::draw(ui, state),
                    PanelKind::Knowledge => knowledge::draw(ui, state),
                    PanelKind::Voice => voice::draw(ui, state),
                    PanelKind::Mcp => mcp::draw(ui, state),
                    PanelKind::Audit => audit::draw(ui, state),
                    PanelKind::ModelConfig => model_config::draw(ui, state),
                    PanelKind::Diagnostics => diagnostics::draw(ui, state),
                    PanelKind::Persona => { persona::draw(ui, state); }
                }
            });
        });
}

fn panel_label(p: PanelKind) -> &'static str {
    match p {
        PanelKind::Modes => "Mode Management",
        PanelKind::Persona => "Persona",
        PanelKind::Memory => "Memory",
        PanelKind::Knowledge => "Knowledge / RAG",
        PanelKind::Voice => "Voice Management",
        PanelKind::Mcp => "MCP Servers",
        PanelKind::Audit => "Audit Log",
        PanelKind::ModelConfig => "Model Configuration",
        PanelKind::Diagnostics => "Diagnostics",
    }
}
