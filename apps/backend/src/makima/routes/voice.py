"""Voice runtime API routes.

Provides endpoints for the frontend to get and update voice call settings.
The actual voice call is handled by the voice-runtime service (LiveKit + Fish Audio).
"""

from __future__ import annotations

from fastapi import APIRouter, Depends
from pydantic import BaseModel, Field

from makima.auth.models import User
from makima.core.deps import get_current_user
from makima_common.logging import get_logger

logger = get_logger(__name__)

router = APIRouter(prefix="/api/voice", tags=["voice"])


# ── Request / Response Models ────────────────────────────────────────


class VoiceSettings(BaseModel):
    """Voice call settings."""

    tts_provider: str | None = Field(None, description="TTS provider name (e.g. 'fish_audio')")
    active_voice_id: str | None = Field(None, description="Active voice model ID")
    push_to_talk: bool | None = Field(None, description="Whether push-to-talk is enabled")
    livekit_url: str | None = Field(None, description="LiveKit server URL")
    available_voices: list[str] = Field(default_factory=list, description="Available voice IDs")


# ── In-memory settings store ─────────────────────────────────────────

_voice_settings: VoiceSettings = VoiceSettings(
    tts_provider="fish_audio",
    active_voice_id=None,
    push_to_talk=True,
    livekit_url=None,
    available_voices=[],
)


# ── Endpoints ────────────────────────────────────────────────────────


@router.get("/settings", response_model=VoiceSettings)
async def get_voice_settings(
    user: User = Depends(get_current_user),
) -> VoiceSettings:
    """Get current voice call settings."""
    return _voice_settings


@router.put("/settings", response_model=VoiceSettings)
async def update_voice_settings(
    settings: VoiceSettings,
    user: User = Depends(get_current_user),
) -> VoiceSettings:
    """Update voice call settings."""
    global _voice_settings

    if settings.tts_provider is not None:
        _voice_settings.tts_provider = settings.tts_provider
    if settings.active_voice_id is not None:
        _voice_settings.active_voice_id = settings.active_voice_id
    if settings.push_to_talk is not None:
        _voice_settings.push_to_talk = settings.push_to_talk
    if settings.livekit_url is not None:
        _voice_settings.livekit_url = settings.livekit_url

    logger.info(
        "Voice settings updated",
        tts_provider=_voice_settings.tts_provider,
        active_voice_id=_voice_settings.active_voice_id,
    )

    return _voice_settings