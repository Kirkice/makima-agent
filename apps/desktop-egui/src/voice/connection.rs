//! LiveKit room connection logic.
//!
//! This module is only compiled when the `voice` feature is enabled.
//! It provides the actual LiveKit connect/disconnect/token-generation
//! logic as free functions that operate on `VoiceManager` (defined in
//! the parent module).

#![cfg(feature = "voice")]

use std::sync::Arc;

use livekit::prelude::*;
use livekit::{Room, RoomOptions};

use super::VoiceManager;

/// Connect a `VoiceManager` to a LiveKit room.
pub async fn connect(vm: &mut VoiceManager) -> Result<(), String> {
    use super::CallStatus;

    if matches!(vm.status, CallStatus::Connected { .. } | CallStatus::Connecting) {
        return Err("Already connected or connecting".into());
    }

    if vm.livekit_url.is_empty() || vm.api_key.is_empty() || vm.api_secret.is_empty() {
        return Err(
            "LiveKit credentials not configured. Set LIVEKIT_URL, LIVEKIT_API_KEY, LIVEKIT_API_SECRET."
                .into(),
        );
    }

    vm.status = CallStatus::Connecting;

    let token = generate_token(vm).map_err(|e| {
        vm.status = CallStatus::Error(format!("Token generation failed: {}", e));
        e
    })?;

    // Connect to the LiveKit room
    let (room, _) = Room::connect(&vm.livekit_url, &token, RoomOptions::default())
        .await
        .map_err(|e| {
            vm.status = CallStatus::Error(format!("Connection failed: {}", e));
            format!("{}", e)
        })?;

    let room = Arc::new(room);
    vm.room = Some(room.clone());

    // Publish local audio track (microphone)
    vm.capture_task = Some(super::audio_capture::start_audio_capture(room.clone()));

    // Start listening for remote audio
    vm.playback_task = Some(super::audio_playback::start_audio_playback(room.clone()));

    let room_name = vm.room_name.clone();
    let participant_count = room.remote_participants().len() + 1;
    vm.status = CallStatus::Connected {
        room_name,
        participant_count,
    };

    tracing::info!("Connected to LiveKit room: {}", vm.room_name);
    Ok(())
}

/// Generate a JWT token for the participant.
fn generate_token(vm: &VoiceManager) -> Result<String, String> {
    use livekit_api::access_token::{AccessToken, VideoGrants};

    let token = AccessToken::with_api_key(&vm.api_key, &vm.api_secret)
        .with_identity("desktop-user")
        .with_name("Desktop User")
        .with_grants(VideoGrants {
            room_join: true,
            room: vm.room_name.clone(),
            ..Default::default()
        })
        .to_jwt()
        .map_err(|e| format!("JWT error: {}", e))?;

    Ok(token)
}