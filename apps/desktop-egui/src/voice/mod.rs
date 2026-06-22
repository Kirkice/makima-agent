//! Voice call management — LiveKit WebRTC integration.
//!
//! This module handles:
//! - Connecting to a LiveKit room
//! - Capturing microphone audio via cpal and publishing to LiveKit
//! - Receiving remote audio from LiveKit and playing it back via cpal
//!
//! When the `voice` feature is **not** enabled, a stub `VoiceManager` is
//! provided so that the GUI compiles and runs without the LiveKit/WebRTC
//! native dependencies (`webrtc-sys`).

#[cfg(feature = "voice")]
pub mod connection;

#[cfg(feature = "voice")]
pub mod audio_capture;

#[cfg(feature = "voice")]
pub mod audio_playback;

use std::sync::Arc;

// ---------------------------------------------------------------------------
// Shared types (always compiled)
// ---------------------------------------------------------------------------

/// Status of the voice call.
#[derive(Debug, Clone, PartialEq)]
pub enum CallStatus {
    Disconnected,
    Connecting,
    Connected { room_name: String, participant_count: usize },
    Error(String),
}

impl std::fmt::Display for CallStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallStatus::Disconnected => write!(f, "Disconnected"),
            CallStatus::Connecting => write!(f, "Connecting..."),
            CallStatus::Connected { room_name, participant_count } => {
                write!(
                    f,
                    "Connected to {} ({} participants)",
                    room_name, participant_count
                )
            }
            CallStatus::Error(e) => write!(f, "Error: {}", e),
        }
    }
}

// ---------------------------------------------------------------------------
// VoiceManager — always compiled, internal fields gated on `voice` feature
// ---------------------------------------------------------------------------

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

    // ----- internal fields (only when voice feature is enabled) -----
    #[cfg(feature = "voice")]
    room: Option<Arc<livekit::Room>>,

    #[cfg(feature = "voice")]
    call_task: Option<tokio::task::JoinHandle<()>>,

    #[cfg(feature = "voice")]
    capture_task: Option<tokio::task::JoinHandle<()>>,

    #[cfg(feature = "voice")]
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

            #[cfg(feature = "voice")]
            room: None,
            #[cfg(feature = "voice")]
            call_task: None,
            #[cfg(feature = "voice")]
            capture_task: None,
            #[cfg(feature = "voice")]
            playback_task: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Real implementation (only when voice feature is enabled)
// ---------------------------------------------------------------------------

#[cfg(feature = "voice")]
impl VoiceManager {
    /// Connect to a LiveKit room.
    pub async fn connect(&mut self) -> Result<(), String> {
        connection::connect(self).await
    }

    /// Disconnect from the current room (synchronous).
    pub fn disconnect(&mut self) {
        // Cancel audio tasks
        if let Some(handle) = self.capture_task.take() {
            handle.abort();
        }
        if let Some(handle) = self.playback_task.take() {
            handle.abort();
        }
        // Drop room (connection closes when Room is dropped)
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
}

// ---------------------------------------------------------------------------
// Stub implementation (when voice feature is NOT enabled)
// ---------------------------------------------------------------------------

#[cfg(not(feature = "voice"))]
impl VoiceManager {
    /// Stub – always returns an error.
    pub async fn connect(&mut self) -> Result<(), String> {
        Err("Voice feature not compiled. Rebuild with `--features voice`.".to_string())
    }

    /// Stub – resets status to Disconnected.
    pub fn disconnect(&mut self) {
        tracing::info!("VoiceManager::disconnect (stub)");
        self.status = CallStatus::Disconnected;
    }

    /// Toggle microphone mute (no-op in stub).
    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        tracing::info!("Microphone muted: {} (stub)", self.muted);
    }
}