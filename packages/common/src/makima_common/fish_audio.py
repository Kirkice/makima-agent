"""Fish Audio async helpers — TTS and ASR via REST API.

Shared between CLI and Voice Runtime.

Required environment variables:
    MAKIMA_FISH_AUDIO_KEY          – Fish Audio API key
    MAKIMA_FISH_AUDIO_REFERENCE_ID – TTS voice model / reference ID

Optional:
    MAKIMA_FISH_AUDIO_BASE_URL     – API base (default: https://api.fish.audio)
"""

from __future__ import annotations

import io
import logging
import os
import wave

import httpx
import numpy as np

logger = logging.getLogger("fish-audio")

_DEFAULT_BASE_URL = "https://api.fish.audio"


def _cfg() -> tuple[str, str, str]:
    return (
        os.getenv("MAKIMA_FISH_AUDIO_KEY", ""),
        os.getenv("MAKIMA_FISH_AUDIO_REFERENCE_ID", ""),
        os.getenv("MAKIMA_FISH_AUDIO_BASE_URL", _DEFAULT_BASE_URL).rstrip("/"),
    )


def pcm_to_wav(pcm: bytes, sample_rate: int, channels: int = 1) -> bytes:
    """Wrap raw int16 PCM bytes into a WAV container."""
    buf = io.BytesIO()
    with wave.open(buf, "wb") as wf:
        wf.setnchannels(channels)
        wf.setsampwidth(2)
        wf.setframerate(sample_rate)
        wf.writeframes(pcm)
    return buf.getvalue()


def wav_to_pcm(wav_bytes: bytes) -> tuple[np.ndarray, int]:
    """Decode WAV bytes → int16 numpy array + sample rate."""
    try:
        buf = io.BytesIO(wav_bytes)
        with wave.open(buf, "rb") as wf:
            sr = wf.getframerate()
            raw = wf.readframes(wf.getnframes())
            return np.frombuffer(raw, dtype=np.int16), sr
    except Exception:
        return np.frombuffer(wav_bytes, dtype=np.int16), 24000


# ────────────────────────────────────────────────────────────────────
#  ASR (Speech → Text)
# ────────────────────────────────────────────────────────────────────

async def transcribe(
    pcm_data: bytes,
    sample_rate: int = 16000,
    language: str = "zh",
    *,
    api_key: str | None = None,
    base_url: str | None = None,
) -> str:
    """Transcribe raw int16 PCM audio to text via Fish Audio ASR."""
    key, _, base = _cfg()
    key = api_key or key
    base = (base_url or base).rstrip("/")

    if not key:
        logger.error("MAKIMA_FISH_AUDIO_KEY is not set")
        return ""

    wav_bytes = pcm_to_wav(pcm_data, sample_rate)

    url = f"{base}/v1/asr"
    headers = {"Authorization": f"Bearer {key}"}
    files = {"audio": ("audio.wav", wav_bytes, "audio/wav")}
    data = {"language": language}

    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            resp = await client.post(url, headers=headers, files=files, data=data)
            if resp.status_code == 200:
                text = resp.json().get("text", "")
                logger.debug("ASR result: %s", text[:80])
                return text
            else:
                logger.warning("ASR failed: HTTP %d — %s", resp.status_code, resp.text[:200])
                return ""
    except Exception as exc:
        logger.error("ASR exception: %s", exc)
        return ""


# ────────────────────────────────────────────────────────────────────
#  TTS (Text → Speech)
# ────────────────────────────────────────────────────────────────────

async def synthesize(
    text: str,
    *,
    api_key: str | None = None,
    reference_id: str | None = None,
    base_url: str | None = None,
) -> tuple[np.ndarray, int] | None:
    """Synthesize text to audio via Fish Audio TTS.

    Returns:
        Tuple of (int16 numpy array, sample_rate), or None on failure.
    """
    key, ref, base = _cfg()
    key = api_key or key
    ref = reference_id or ref
    base = (base_url or base).rstrip("/")

    if not key:
        logger.error("MAKIMA_FISH_AUDIO_KEY is not set")
        return None
    if not ref:
        logger.error("MAKIMA_FISH_AUDIO_REFERENCE_ID is not set")
        return None

    url = f"{base}/v1/tts"
    headers = {
        "Authorization": f"Bearer {key}",
        "Content-Type": "application/json",
    }
    payload = {
        "text": text,
        "reference_id": ref,
        "format": "wav",
        "normalize": True,
        "latency": "normal",
    }

    try:
        async with httpx.AsyncClient(timeout=60.0) as client:
            resp = await client.post(url, json=payload, headers=headers)
            if resp.status_code == 200:
                pcm, sr = wav_to_pcm(resp.content)
                logger.debug("TTS OK: %d bytes, sr=%d", len(resp.content), sr)
                return pcm, sr
            else:
                logger.error("TTS failed: HTTP %d — %s", resp.status_code, resp.text[:200])
                return None
    except Exception as exc:
        logger.error("TTS exception: %s", exc)
        return None


# ────────────────────────────────────────────────────────────────────
#  Sync wrapper (for CLI)
# ────────────────────────────────────────────────────────────────────

def synthesize_sync(text: str) -> tuple[bytes, int] | None:
    """Sync wrapper for TTS — returns (audio_bytes, sample_rate) or None.

    Intended for use from threads (e.g. CLI's background TTS thread).
    """
    import asyncio

    try:
        loop = asyncio.get_event_loop()
        if loop.is_running():
            import concurrent.futures
            with concurrent.futures.ThreadPoolExecutor() as pool:
                return pool.submit(asyncio.run, synthesize(text)).result()
        else:
            result = asyncio.run(synthesize(text))
    except RuntimeError:
        result = asyncio.run(synthesize(text))

    if result is None:
        return None
    pcm, sr = result
    return pcm.astype(np.int16).tobytes(), sr