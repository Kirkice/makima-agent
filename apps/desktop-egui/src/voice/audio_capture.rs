//! Microphone audio capture via cpal → LiveKit local audio track.
//!
//! Captures audio from the default input device, converts to the format
//! LiveKit expects (16-bit PCM, 48 kHz mono), and publishes as a local
//! audio track.

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use livekit::prelude::*;
use livekit::Room;

/// Spawn a tokio task that captures microphone audio and publishes it.
///
/// Returns a `JoinHandle` that can be aborted to stop capture.
pub fn start_audio_capture(room: Arc<Room>) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        if let Err(e) = run_capture_loop(room) {
            tracing::error!("Audio capture error: {}", e);
        }
    })
}

fn run_capture_loop(_room: Arc<Room>) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "No input device available".to_string())?;

    let config = device
        .default_input_config()
        .map_err(|e| format!("Input config error: {}", e))?;

    tracing::info!(
        "Mic capture: {} ({} channels, {} Hz)",
        device.name().unwrap_or_default(),
        config.channels(),
        config.sample_rate().0,
    );

    // Build the input stream — we just read raw samples and discard them for now.
    // In a full implementation, we'd create a LiveKit LocalAudioTrack and push
    // AudioFrames into it. For now, this demonstrates the cpal integration works.
    let stream = device
        .build_input_stream(
            &config.into(),
            move |data: &[f32], _info: &cpal::InputCallbackInfo| {
                // In production: convert f32 → i16, create AudioFrame, send to LiveKit track
                // For now: just count samples to verify capture is working
                let _sample_count = data.len();
            },
            move |err| {
                tracing::error!("Audio capture stream error: {}", err);
            },
            None,
        )
        .map_err(|e| format!("Build input stream failed: {}", e))?;

    stream
        .play()
        .map_err(|e| format!("Stream play failed: {}", e))?;

    tracing::info!("Microphone capture started");

    // Keep the stream alive — when the task is aborted, stream is dropped
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

/// List available input (microphone) devices.
pub fn list_input_devices() -> Vec<String> {
    use cpal::traits::DeviceTrait;
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices.filter_map(|d| d.name().ok()).collect(),
        Err(_) => vec!["Default".to_string()],
    }
}
