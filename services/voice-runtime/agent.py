"""Makima Voice Agent — Simple voice loop using LiveKit RTC.

Architecture:
  Audio input  → Simple silence detection (RMS) → Fish Audio ASR
  Backend      → POST /tasks (SSE) → collect message chunks
  Audio output → Fish Audio TTS → publish back to LiveKit room

This is NOT an independent agent. It's a real-time voice interface to the
same Chat backend that the CLI and future frontends use.
"""

import asyncio
import json
import logging
import os
import struct
import sys
from pathlib import Path

import httpx
import numpy as np
from dotenv import load_dotenv
from livekit import api, rtc

from fish_audio import transcribe, synthesize

# Load environment
_env_path = Path(__file__).parent.parent.parent / "apps" / "backend" / ".env"
load_dotenv(_env_path)

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("makima-voice-agent")


class VoiceLoop:
    """Simple voice loop: listen → ASR → backend → TTS → speak."""

    def __init__(self, room: rtc.Room, backend_url: str, token: str):
        self.room = room
        self.backend_url = backend_url.rstrip("/")
        self.token = token
        self.session_id: str = ""

        # Audio settings
        self.sample_rate = 24000  # Match Fish Audio output
        self.channels = 1

        # Silence detection (RMS-based)
        self.silence_threshold = 500  # RMS threshold for int16 audio
        self.silence_duration = 1.5  # seconds of silence to trigger
        self.min_speech_duration = 0.3  # minimum speech to process

        # State
        self.audio_buffer: list[bytes] = []
        self.silence_start: float | None = None
        self.speech_started = False
        self.is_speaking = False

    async def create_session(self) -> bool:
        """Create a backend session for voice chat."""
        try:
            async with httpx.AsyncClient(timeout=10.0) as client:
                resp = await client.post(
                    f"{self.backend_url}/sessions",
                    json={"title": "Voice Chat"},
                    headers={"Authorization": f"Bearer {self.token}"},
                )
                if resp.status_code in (200, 201):
                    self.session_id = resp.json()["id"]
                    logger.info("Session created: %s", self.session_id)
                    return True
                else:
                    logger.error("Failed to create session: %d", resp.status_code)
                    return False
        except Exception as e:
            logger.error("Session creation error: %s", e)
            return False

    def rms(self, audio_bytes: bytes) -> float:
        """Calculate RMS of int16 audio."""
        if len(audio_bytes) < 2:
            return 0.0
        samples = np.frombuffer(audio_bytes, dtype=np.int16)
        return float(np.sqrt(np.mean(samples.astype(float) ** 2)))

    async def process_utterance(self, audio_data: bytes) -> None:
        """Process a complete utterance: ASR → backend → TTS."""
        duration = len(audio_data) / (2 * self.sample_rate)  # int16 = 2 bytes
        if duration < self.min_speech_duration:
            logger.debug("Ignoring short audio: %.2fs", duration)
            return

        # 1. ASR
        logger.info("Processing utterance (%.2fs)...", duration)
        text = await transcribe(audio_data, sample_rate=self.sample_rate, language="zh")
        if not text or not text.strip():
            logger.debug("ASR returned empty text")
            return

        logger.info("User said: %s", text)

        # 2. Backend /tasks (SSE)
        await self.call_backend(text)

    async def call_backend(self, text: str) -> None:
        """Call backend /tasks API and stream response to TTS."""
        self.is_speaking = True
        sentence_buffer = ""

        try:
            async with httpx.AsyncClient(timeout=60.0) as client:
                async with client.stream(
                    "POST",
                    f"{self.backend_url}/tasks",
                    json={"session_id": self.session_id, "input_text": text},
                    headers={"Authorization": f"Bearer {self.token}"},
                ) as resp:
                    if resp.status_code != 200:
                        logger.error("Backend error: %d", resp.status_code)
                        self.is_speaking = False
                        return

                    event_type = ""
                    async for line in resp.aiter_lines():
                        if not line:
                            continue
                        if line.startswith("event:"):
                            event_type = line[6:].strip()
                            continue
                        if not line.startswith("data:"):
                            continue

                        try:
                            payload = json.loads(line[5:].strip())
                        except json.JSONDecodeError:
                            continue

                        data = payload.get("data", {})

                        if event_type == "message":
                            content = data.get("content", "")
                            if content:
                                sentence_buffer += content
                                # Check for sentence boundary
                                if any(sentence_buffer.rstrip().endswith(c) for c in "。！？!?"):
                                    await self.speak(sentence_buffer.strip())
                                    sentence_buffer = ""

                        elif event_type == "error":
                            error_msg = data.get("error", "Unknown error")
                            logger.error("Backend error: %s", error_msg)
                            await self.speak(f"出了点问题：{error_msg}")
                            sentence_buffer = ""

                    # Speak any remaining text
                    if sentence_buffer.strip():
                        await self.speak(sentence_buffer.strip())

        except Exception as e:
            logger.error("Backend call error: %s", e)

        self.is_speaking = False

    async def speak(self, text: str) -> None:
        """Synthesize and play audio."""
        if not text:
            return

        logger.info("Speaking: %s", text[:50])
        result = await synthesize(text)
        if result is None:
            logger.error("TTS failed")
            return

        pcm, sr = result
        audio_bytes = pcm.astype(np.int16).tobytes()

        # Publish audio to room
        # Note: In a real implementation, you'd create an AudioSource and publish frames
        # For now, we'll just log it
        logger.info("Audio synthesized: %d bytes, %d Hz", len(audio_bytes), sr)
        # TODO: Actually publish to LiveKit room
        # This requires creating a LocalAudioTrack and publishing it

    async def on_audio_frame(self, frame: rtc.AudioFrame) -> None:
        """Process incoming audio frame."""
        if self.is_speaking:
            return  # Don't listen while speaking

        audio_bytes = frame.data
        rms = self.rms(audio_bytes)

        if rms > self.silence_threshold:
            # Speech detected
            self.audio_buffer.append(audio_bytes)
            self.silence_start = None
            self.speech_started = True
        else:
            # Silence
            if self.speech_started:
                if self.silence_start is None:
                    self.silence_start = asyncio.get_event_loop().time()
                else:
                    elapsed = asyncio.get_event_loop().time() - self.silence_start
                    if elapsed >= self.silence_duration:
                        # End of utterance
                        if self.audio_buffer:
                            audio_data = b"".join(self.audio_buffer)
                            self.audio_buffer = []
                            await self.process_utterance(audio_data)
                        self.speech_started = False
                        self.silence_start = None

    async def run(self) -> None:
        """Main loop."""
        # Create session
        if not await self.create_session():
            return

        # Greet
        await self.speak("你好，我是玛奇玛。有什么可以帮你的吗？")

        # Listen for audio
        logger.info("Listening...")

        # In LiveKit RTC, you'd subscribe to tracks and process frames
        # This is a simplified version
        while True:
            await asyncio.sleep(0.1)
            # In real implementation: process incoming audio frames


async def main():
    # Config
    livekit_url = os.getenv("LIVEKIT_URL", "")
    livekit_api_key = os.getenv("LIVEKIT_API_KEY", "")
    livekit_api_secret = os.getenv("LIVEKIT_API_SECRET", "")
    backend_url = os.getenv("MAKIMA_BACKEND_URL", "http://127.0.0.1:8000")
    cli_username = os.getenv("MAKIMA_CLI_USERNAME", "makima-voice")
    cli_password = os.getenv("MAKIMA_CLI_PASSWORD", "makima-voice")

    if not all([livekit_url, livekit_api_key, livekit_api_secret]):
        logger.error("Missing LiveKit credentials")
        return

    # Authenticate with backend
    logger.info("Authenticating with backend...")
    async with httpx.AsyncClient(timeout=10.0) as client:
        # Try login
        resp = await client.post(
            f"{backend_url}/auth/login",
            json={"username": cli_username, "password": cli_password},
        )
        if resp.status_code in (401, 404):
            # Register first
            resp = await client.post(
                f"{backend_url}/auth/register",
                json={
                    "username": cli_username,
                    "email": f"{cli_username}@voice.local",
                    "password": cli_password,
                },
            )

        if resp.status_code not in (200, 201):
            logger.error("Backend auth failed: %d", resp.status_code)
            return

        token = resp.json()["access_token"]
        logger.info("Authenticated successfully")

    # Create LiveKit room
    room_name = "makima-voice-room"
    room = rtc.Room()

    # Generate token
    token_jwt = (
        api.AccessToken(api_key=livekit_api_key, api_secret=livekit_api_secret)
        .with_identity("makima-voice-agent")
        .with_name("Makima Voice Agent")
        .with_grants(api.VideoGrants(room_join=True, room=room_name))
        .to_jwt()
    )

    # Connect
    logger.info("Connecting to LiveKit room: %s", room_name)
    await room.connect(livekit_url, token_jwt)
    logger.info("Connected to room")

    # Create and run voice loop
    voice_loop = VoiceLoop(room, backend_url, token)
    await voice_loop.run()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        logger.info("Shutting down...")
        sys.exit(0)