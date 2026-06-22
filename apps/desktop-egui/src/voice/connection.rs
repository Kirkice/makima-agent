//! LiveKit room connection manager.
//!
//! Manages the lifecycle of a WebRTC connection to a LiveKit room:
//! connect → publish mic track → subscribe remote tracks → disconnect.

use std::sync::Arc;

use livekit::prelude::*;
use livekit::room::{Room, RoomOptions};

use crate::voice::audio_capture::start_audio_capture;
use crate::voice::audio_playback::start_audio_playback;

/// Status of the voice call.
#[derive(Debug, Clone, PartialEq)]
pub enum CallStatus {
    Disconnected,
    Connecting,
    Connected { room_name: String, participant_count: usize },
    Error(String),
}

/// Manages a LiveKit voice call session.
pub struct VoiceManager {
    /// Current call status
    pub status: CallStatus,
    /// Whether the local microphone is muted
    pub muted: bool,
    /// Room name to join (editable by user)
    pub room_name: String,
    /// LiveKit connection URL
    pub livekit_url: String,
    /// LiveKit API key (for token generation)
    pub api_key: String,
    /// LiveKit API secret (for token generation)
    pub api_secret: String,

    // Internal state
    room: Option<Arc<Room>>,
    /// Handle to the async task that runs the call
    call_task: Option<tokio::task::JoinHandle<()>>,
    /// Handle to audio capture task
    capture_task: Option<tokio::task::JoinHandle<()>>,
    /// Handle to audio playback task
    playback_task: Option<tokio::task::JoinHandle<()>>,
}

impl Default for VoiceManager {
    fn default() -> Self {
        Self {
            status: CallStatus::Disconnected,
            muted: false,
            room_name: "makima-voice-room".to_string(),
            livekit_url: String::new(),
            api_key: String::new(),
            api_secret: String::new(),
            room: None,
            call_task: None,
            capture_task: None,
            playback_task: None,
        }
    }
}

impl VoiceManager {
    /// Connect to a LiveKit room.
    ///
    /// This generates a JWT token, connects to the room, publishes a local
    /// audio track (microphone), and starts listening for remote audio.
    pub async fn connect(&mut self) -> Result<(), String> {
        if matches!(self.status, CallStatus::Connected { .. } | CallStatus::Connecting) {
            return Err("Already connected or connecting".into());
        }

        if self.livekit_url.is_empty() || self.api_key.is_empty() || self.api_secret.is_empty() {
            return Err("LiveKit credentials not configured. Set LIVEKIT_URL, LIVEKIT_API_KEY, LIVEKIT_API_SECRET.".into());
        }

        self.status = CallStatus::Connecting;

        // Generate a participant token
        let token = match self.generate_token() {
            Ok(t) => t,
            Err(e) => {
                self.status = CallStatus::Error(format!("Token generation failed: {}", e));
                return Err(self.status.clone().to_string());
            }
        };

        // Connect to the LiveKit room
        let room = match Room::connect(&self.livekit_url, &token, RoomOptions::default()).await {
            Ok((room, _)) => room,
            Err(e) => {
                self.status = CallStatus::Error(format!("Connection failed: {}", e));
                return Err(self.status.clone().to_string());
            }
        };

        let room = Arc::new(room);
        self.room = Some(room.clone());

        // Publish local audio track (microphone)
        self.capture_task = Some(start_audio_capture(room.clone()));

        // Start listening for remote audio
        self.playback_task = Some(start_audio_playback(room.clone()));

        let room_name = self.room_name.clone();
        let participant_count = room.remote_participants().len() + 1;
        self.status = CallStatus::Connected {
            room_name,
            participant_count,
        };

        tracing::info!("Connected to LiveKit room: {}", self.room_name);
        Ok(())
    }

    /// Disconnect from the current room (synchronous — aborts tasks, drops room).
    pub fn disconnect(&mut self) {
        // Cancel audio tasks
        if let Some(handle) = self.capture_task.take() {
            handle.abort();
        }
        if let Some(handle) = self.playback_task.take() {
            handle.abort();
        }

        // Drop room (connection will close when Room is dropped)
        self.room = None;

        self.status = CallStatus::Disconnected;
        tracing::info!("Disconnected from LiveKit room");
    }

    /// Toggle microphone mute.
    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        // TODO: actually mute the local audio track
        tracing::info!("Microphone muted: {}", self.muted);
    }

    /// Generate a JWT token for the participant.
    fn generate_token(&self) -> Result<String, String> {
        use livekit_api::access_token::{AccessToken, VideoGrants};

        let token = AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_identity("desktop-user")
            .with_name("Desktop User")
            .with_grants(VideoGrants {
                room_join: true,
                room: self.room_name.clone(),
                ..Default::default()
            })
            .to_jwt()
            .map_err(|e| format!("JWT error: {}", e))?;

        Ok(token)
    }
}

impl std::fmt::Display for CallStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallStatus::Disconnected => write!(f, "Disconnected"),
            CallStatus::Connecting => write!(f, "Connecting..."),
            CallStatus::Connected { room_name, participant_count } => {
                write!(f, "Connected to {} ({} participants)", room_name, participant_count)
            }
            CallStatus::Error(e) => write!(f, "Error: {}", e),
        }
    }
}