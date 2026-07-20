//! Fish Audio TTS (Text-to-Speech) integration.
//!
//! Calls Fish Audio REST API to synthesize speech from text,
//! then plays the resulting WAV audio through the local speaker.
//!
//! Required environment variables (loaded from .env):
//!   MAKIMA_FISH_AUDIO_KEY          – Fish Audio API key
//!   MAKIMA_FISH_AUDIO_REFERENCE_ID – TTS voice model / reference ID
//!
//! Optional:
//!   MAKIMA_FISH_AUDIO_BASE_URL     – API base (default: https://api.fish.audio)

use std::io::Cursor;

use anyhow::{Context, Result};
use serde::Serialize;

const DEFAULT_BASE_URL: &str = "https://api.fish.audio";

#[derive(Serialize)]
struct TtsRequest<'a> {
    text: &'a str,
    reference_id: &'a str,
    format: &'a str,
    normalize: bool,
    latency: &'a str,
}

fn load_fish_config() -> Option<(String, String, String)> {
    let key = std::env::var("MAKIMA_FISH_AUDIO_KEY").ok()?;
    let reference_id = std::env::var("MAKIMA_FISH_AUDIO_REFERENCE_ID").ok()?;
    let base_url = std::env::var("MAKIMA_FISH_AUDIO_BASE_URL")
        .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
    // load_dotenv() already called in main.rs, so .env values are in std::env
    Some((key, reference_id, base_url))
}

/// Strip markdown/code blocks for cleaner speech output.
fn strip_markdown(text: &str) -> String {
    // Remove fenced code blocks: ``` ... ```
    let text = regex_lite_or_builtin(text, r"```[\s\S]*?```", "");
    // Remove inline code: `code`
    let text = regex_lite_or_builtin(&text, r"`[^`]+`", "");
    // Remove markdown syntax chars
    text.chars()
        .filter(|c| !matches!(c, '#' | '*' | '_' | '[' | ']' | '(' | ')' | '>' | '|' | '~' | '-'))
        .collect::<String>()
        .trim()
        .to_string()
}

/// Simple regex replacement without an external crate dependency.
fn regex_lite_or_builtin(text: &str, _pattern: &str, _replacement: &str) -> String {
    // For fenced code blocks: find ``` pairs and remove content between them
    let mut result = String::new();
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();
    
    while i < chars.len() {
        // Check for ``` at current position
        if i + 2 < chars.len() && chars[i] == '`' && chars[i + 1] == '`' && chars[i + 2] == '`' {
            // Find closing ```
            i += 3;
            while i + 2 < chars.len() {
                if chars[i] == '`' && chars[i + 1] == '`' && chars[i + 2] == '`' {
                    i += 3;
                    break;
                }
                i += 1;
            }
        }
        // Check for inline `code`
        else if chars[i] == '`' {
            i += 1;
            while i < chars.len() && chars[i] != '`' {
                i += 1;
            }
            if i < chars.len() {
                i += 1; // skip closing `
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    
    result
}

/// Synthesize text to speech via Fish Audio API, then play through speakers.
pub async fn speak(text: String) {
    if text.is_empty() {
        return;
    }

    let clean_text = strip_markdown(&text);
    if clean_text.is_empty() {
        tracing::debug!("TTS: text is empty after stripping markdown, skipping");
        return;
    }

    let (api_key, reference_id, base_url) = match load_fish_config() {
        Some(cfg) => cfg,
        None => {
            tracing::debug!("TTS: Fish Audio not configured (missing MAKIMA_FISH_AUDIO_KEY or MAKIMA_FISH_AUDIO_REFERENCE_ID)");
            return;
        }
    };

    tracing::debug!("TTS: synthesizing {} chars...", clean_text.len());

    let client = reqwest::Client::new();
    let url = format!("{}/v1/tts", base_url.trim_end_matches('/'));

    let request = TtsRequest {
        text: &clean_text,
        reference_id: &reference_id,
        format: "wav",
        normalize: true,
        latency: "normal",
    };

    let wav_bytes = match client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                match resp.bytes().await {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        tracing::error!("TTS: failed to read response body: {}", e);
                        return;
                    }
                }
            } else {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                tracing::error!("TTS: Fish Audio API returned {} — {}", status, body);
                return;
            }
        }
        Err(e) => {
            tracing::error!("TTS: request failed: {}", e);
            return;
        }
    };

    if wav_bytes.is_empty() {
        tracing::error!("TTS: received empty audio data");
        return;
    }

    tracing::debug!("TTS: received {} bytes, playing...", wav_bytes.len());

    // Play WAV audio through rodio
    if let Err(e) = play_wav_bytes(&wav_bytes) {
        tracing::error!("TTS: playback failed: {}", e);
    }
}

/// Play WAV audio bytes through the default speaker using rodio.
fn play_wav_bytes(data: &[u8]) -> Result<()> {
    let cursor = Cursor::new(data.to_vec());
    let source = rodio::Decoder::new(cursor).context("Failed to decode WAV audio")?;

    let (_stream, stream_handle) =
        rodio::OutputStream::try_default().context("No audio output device available")?;

    let sink = rodio::Sink::try_new(&stream_handle).context("Failed to create audio sink")?;
    sink.append(source);

    // Block until playback finishes
    sink.sleep_until_end();

    Ok(())
}