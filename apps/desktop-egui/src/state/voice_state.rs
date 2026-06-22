//! Voice call UI state — tracks call status, device selections, etc.
//!
//! This is the *UI-facing* voice state. The actual LiveKit connection
//! is managed by `voice::connection::VoiceManager` in `app.rs`.
//! State here is read/written from the UI thread and used to render
//! the voice panel.

/// Voice call state visible to the UI.
pub struct VoiceCallState {
    /// Current call status text (e.g. "Disconnected", "Connected to room-xxx")
    pub status: String,
    /// Whether currently in a call
    pub is_connected: bool,
    /// Whether currently connecting
    pub is_connecting: bool,
    /// Error message if last connect failed
    pub error: Option<String>,
    /// Whether local mic is muted
    pub muted: bool,
    /// Room name input
    pub room_name: String,
    /// LiveKit URL (from settings / env)
    pub livekit_url: String,
    /// LiveKit API key
    pub api_key: String,
    /// LiveKit API secret
    pub api_secret: String,
    /// Selected mic device name
    pub selected_mic: String,
    /// Selected speaker device name
    pub selected_speaker: String,
    /// Available mic devices
    pub available_mics: Vec<String>,
    /// Available speaker devices
    pub available_speakers: Vec<String>,
    /// Call duration in seconds (updated by a timer)
    pub call_duration_secs: u64,
    /// Mic level (0.0 – 1.0) for visualizer
    pub mic_level: f32,
    /// Speaker level (0.0 – 1.0) for visualizer
    pub speaker_level: f32,
}

impl Default for VoiceCallState {
    fn default() -> Self {
        Self {
            status: "Disconnected".to_string(),
            is_connected: false,
            is_connecting: false,
            error: None,
            muted: false,
            room_name: "makima-voice-room".to_string(),
            livekit_url: String::new(),
            api_key: String::new(),
            api_secret: String::new(),
            selected_mic: "Default".to_string(),
            selected_speaker: "Default".to_string(),
            available_mics: vec!["Default".to_string()],
            available_speakers: vec!["Default".to_string()],
            call_duration_secs: 0,
            mic_level: 0.0,
            speaker_level: 0.0,
        }
    }
}