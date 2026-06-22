//! Remote audio playback via cpal ← LiveKit remote audio tracks.
//!
//! Subscribes to remote participants' audio tracks and plays them back
//! through the default output device.

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use livekit::prelude::*;
use livekit::room::Room;

/// Spawn a tokio task that plays back remote audio from LiveKit.
///
/// Returns a `JoinHandle` that can be aborted to stop playback.
pub fn start_audio_playback(room: Arc<Room>) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        if let Err(e) = run_playback_loop(room) {
            tracing::error!("Audio playback error: {}", e);
        }
    })
}

fn run_playback_loop(room: Arc<Room>) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "No output device available".to_string())?;

    let config = device
        .default_output_config()
        .map_err(|e| format!("Output config error: {}", e))?;

    tracing::info!(
        "Speaker playback: {} ({} channels, {} Hz)",
        device.name().unwrap_or_default(),
        config.channels(),
        config.sample_rate().0,
    );

    // Build the output stream
    // In a full implementation, we'd subscribe to RemoteAudioTracks and push
    // their AudioFrames into a ring buffer that this stream reads from.
    // For now, we just output silence to demonstrate the cpal integration.
    let sample_rate = config.sample_rate().0 as usize;
    let channels = config.channels() as usize;

    let stream = device
        .build_output_stream(
            &config.into(),
            move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                // Output silence — in production, read from a ring buffer
                // fed by LiveKit remote audio track callbacks
                for sample in data.iter_mut() {
                    *sample = 0.0;
                }
            },
            move |err| {
                tracing::error!("Audio playback stream error: {}", err);
            },
            None,
        )
        .map_err(|e| format!("Build output stream failed: {}", e))?;

    stream
        .play()
        .map_err(|e| format!("Stream play failed: {}", e))?;

    tracing::info!("Speaker playback started");

    // Keep the stream alive — when the task is aborted, stream is dropped
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

/// List available output (speaker) devices.
pub fn list_output_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices()
        .into_iter()
        .filter_map(|d| d.name().ok())
        .collect()
}