use crate::state::app_state::{ApiCommand, AppState};
use crate::theme::colors;
use eframe::egui;

/// Voice call management panel — LiveKit WebRTC integration.
pub fn draw(ui: &mut egui::Ui, state: &mut AppState) {
    let vc = &mut state.voice_call;

    ui.colored_label(colors::RED_ACCENT, "📞 Voice Call");
    ui.separator();
    ui.add_space(8.0);

    // ── Connection Status ──────────────────────────────────────────────
    egui::Frame::none()
        .fill(colors::GRAPHITE_BG)
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (icon, label_color) = if vc.is_connected {
                    ("●", colors::SUCCESS)
                } else if vc.is_connecting {
                    ("◌", egui::Color32::from_rgb(255, 180, 50))
                } else {
                    ("○", colors::TEXT_MUTED)
                };
                ui.colored_label(label_color, icon);
                ui.colored_label(colors::TEXT_PRIMARY, &vc.status);
            });

            if vc.is_connected {
                let mins = vc.call_duration_secs / 60;
                let secs = vc.call_duration_secs % 60;
                ui.colored_label(
                    colors::TEXT_SECONDARY,
                    format!("Duration: {:02}:{:02}", mins, secs),
                );
            }

            if let Some(err) = &vc.error {
                ui.colored_label(egui::Color32::from_rgb(255, 100, 100), format!("⚠ {}", err));
            }
        });

    ui.add_space(12.0);

    // ── Room Configuration ─────────────────────────────────────────────
    ui.colored_label(colors::TEXT_SECONDARY, "Room");
    ui.add(
        egui::TextEdit::singleline(&mut vc.room_name)
            .hint_text("makima-voice-room")
            .desired_width(f32::INFINITY),
    );

    ui.add_space(8.0);
    ui.colored_label(colors::TEXT_SECONDARY, "LiveKit Credentials");
    ui.horizontal(|ui| {
        ui.label("URL:");
        ui.add(
            egui::TextEdit::singleline(&mut vc.livekit_url)
                .hint_text("wss://your-project.livekit.cloud")
                .desired_width(f32::INFINITY),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Key:");
        ui.add(
            egui::TextEdit::singleline(&mut vc.api_key)
                .hint_text("api-key")
                .password(true)
                .desired_width(f32::INFINITY),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Secret:");
        ui.add(
            egui::TextEdit::singleline(&mut vc.api_secret)
                .hint_text("api-secret")
                .password(true)
                .desired_width(f32::INFINITY),
        );
    });

    ui.add_space(16.0);

    // ── Call Controls ──────────────────────────────────────────────────
    ui.horizontal(|ui| {
        if vc.is_connected {
            // End Call button (red)
            let end_btn = egui::Button::new("🔇  End Call")
                .fill(egui::Color32::from_rgb(180, 50, 50))
                .min_size(egui::vec2(120.0, 36.0));
            if ui.add(end_btn).clicked() {
                vc.is_connected = false;
                vc.is_connecting = false;
                vc.status = "Disconnecting...".to_string();
                vc.call_duration_secs = 0;
                state.api_commands.push(ApiCommand::StopVoiceCall);
            }

            // Mute toggle
            let mute_label = if vc.muted { "🔇 Unmute" } else { "🎙️ Mute" };
            if ui.button(mute_label).clicked() {
                vc.muted = !vc.muted;
                state.api_commands.push(ApiCommand::ToggleVoiceMute);
            }
        } else if vc.is_connecting {
            let connecting_btn = egui::Button::new("⏳ Connecting...")
                .fill(egui::Color32::from_rgb(100, 100, 100))
                .min_size(egui::vec2(120.0, 36.0));
            ui.add_enabled(false, connecting_btn);
        } else {
            // Start Call button (green)
            let call_btn = egui::Button::new("📞  Start Call")
                .fill(egui::Color32::from_rgb(50, 150, 80))
                .min_size(egui::vec2(120.0, 36.0));
            if ui.add(call_btn).clicked() {
                vc.is_connecting = true;
                vc.error = None;
                vc.status = "Connecting...".to_string();
                state.api_commands.push(ApiCommand::StartVoiceCall {
                    room_name: vc.room_name.clone(),
                    livekit_url: vc.livekit_url.clone(),
                    api_key: vc.api_key.clone(),
                    api_secret: vc.api_secret.clone(),
                });
            }
        }
    });

    ui.add_space(20.0);

    // ── Audio Devices ──────────────────────────────────────────────────
    ui.colored_label(colors::TEXT_SECONDARY, "Audio Devices");
    ui.separator();
    ui.add_space(4.0);

    // Microphone selection
    ui.horizontal(|ui| {
        ui.label("🎙️ Mic:");
        egui::ComboBox::from_id_salt("mic_device")
            .selected_text(&vc.selected_mic)
            .show_ui(ui, |ui| {
                for dev in &vc.available_mics {
                    ui.selectable_value(&mut vc.selected_mic, dev.clone(), dev);
                }
            });
        if ui.small_button("Refresh").clicked() {
            // Re-enumerate devices
            vc.available_mics = crate::voice::audio_capture::list_input_devices();
            if vc.available_mics.is_empty() {
                vc.available_mics.push("Default".to_string());
            }
        }
    });

    // Mic level indicator
    ui.horizontal(|ui| {
        ui.label("Level:");
        let bar_width = 150.0 * vc.mic_level;
        let bar = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(150.0, 12.0),
        );
        ui.painter().rect_filled(bar, egui::Rounding::same(3.0), egui::Color32::from_rgb(60, 60, 60));
        if bar_width > 0.0 {
            let filled = egui::Rect::from_min_size(
                bar.min,
                egui::vec2(bar_width, 12.0),
            );
            ui.painter().rect_filled(filled, egui::Rounding::same(3.0), egui::Color32::from_rgb(80, 200, 120));
        }
        ui.advance_cursor_after_rect(bar);
    });

    ui.add_space(4.0);

    // Speaker selection
    ui.horizontal(|ui| {
        ui.label("🔊 Speaker:");
        egui::ComboBox::from_id_salt("speaker_device")
            .selected_text(&vc.selected_speaker)
            .show_ui(ui, |ui| {
                for dev in &vc.available_speakers {
                    ui.selectable_value(&mut vc.selected_speaker, dev.clone(), dev);
                }
            });
        if ui.small_button("Refresh").clicked() {
            vc.available_speakers = crate::voice::audio_playback::list_output_devices();
            if vc.available_speakers.is_empty() {
                vc.available_speakers.push("Default".to_string());
            }
        }
    });

    // Speaker level indicator
    ui.horizontal(|ui| {
        ui.label("Level:");
        let bar_width = 150.0 * vc.speaker_level;
        let bar = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(150.0, 12.0),
        );
        ui.painter().rect_filled(bar, egui::Rounding::same(3.0), egui::Color32::from_rgb(60, 60, 60));
        if bar_width > 0.0 {
            let filled = egui::Rect::from_min_size(
                bar.min,
                egui::vec2(bar_width, 12.0),
            );
            ui.painter().rect_filled(filled, egui::Rounding::same(3.0), egui::Color32::from_rgb(80, 150, 220));
        }
        ui.advance_cursor_after_rect(bar);
    });

    ui.add_space(20.0);

    // ── TTS Voice Settings ─────────────────────────────────────────────
    ui.colored_label(colors::TEXT_SECONDARY, "TTS Voice Settings");
    ui.separator();
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Provider:");
        ui.colored_label(colors::TEXT_PRIMARY, &state.settings.voice_config.tts_provider);
    });
    if let Some(vid) = &state.settings.voice_config.active_voice_id {
        ui.horizontal(|ui| {
            ui.label("Voice ID:");
            ui.colored_label(colors::TEXT_PRIMARY, vid);
        });
    }

    ui.add_space(8.0);
    if ui.button("🔄 Load Voice Settings").clicked() {
        state.api_commands.push(ApiCommand::FetchVoiceSettings);
    }
}