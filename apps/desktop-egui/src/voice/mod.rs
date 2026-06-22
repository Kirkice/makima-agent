//! Voice call management — LiveKit WebRTC integration.
//!
//! This module handles:
//! - Connecting to a LiveKit room
//! - Capturing microphone audio via cpal and publishing to LiveKit
//! - Receiving remote audio from LiveKit and playing it back via cpal

pub mod connection;
pub mod audio_capture;
pub mod audio_playback;

pub use connection::VoiceManager;