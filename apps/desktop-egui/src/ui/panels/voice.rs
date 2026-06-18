use crate::state::app_state::AppState;
use crate::theme::colors;
use eframe::egui;

/// Voice management panel (Phase 3)
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    ui.colored_label(colors::RED_ACCENT, "Voice Management");
    ui.separator();
    ui.add_space(8.0);

    let tabs = ["Voice Chat", "TTS Voices", "Audio Devices", "Speech Diagnostics"];
    let tab_idx = &mut state.voice_tab_index;
    if *tab_idx >= tabs.len() { *tab_idx = 0; }

    ui.horizontal(|ui| {
        for (i, tab) in tabs.iter().enumerate() {
            if ui.selectable_label(*tab_idx == i, *tab).clicked() {
                *tab_idx = i;
            }
        }
    });

    ui.add_space(8.0);

    match *tab_idx {
        0 => { // Voice Chat
            ui.colored_label(colors::TEXT_SECONDARY, "Voice Conversation");
            metric_row(ui, "Push-to-Talk", if state.settings.voice_config.push_to_talk { "Enabled" } else { "Always Listen" });
            if ui.button("Toggle PTT").clicked() { state.settings.voice_config.push_to_talk = !state.settings.voice_config.push_to_talk; }
        }
        1 => { // TTS Voices
            ui.colored_label(colors::TEXT_SECONDARY, "TTS Provider");
            ui.label(&state.settings.voice_config.tts_provider);
            if ui.button("Test Voice").clicked() { state.set_status("Testing TTS...".to_string()); }
        }
        2 => { // Audio Devices
            metric_row(ui, "Mic", state.settings.voice_config.mic_device.as_deref().unwrap_or("Default"));
            metric_row(ui, "Speaker", state.settings.voice_config.speaker_device.as_deref().unwrap_or("Default"));
        }
        3 => { // Speech Diagnostics
            ui.colored_label(colors::TEXT_MUTED, "No diagnostics data");
        }
        _ => {}
    }
}

fn metric_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(colors::TEXT_SECONDARY, label);
        ui.colored_label(colors::TEXT_PRIMARY, value);
    });
}