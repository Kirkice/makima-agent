"""Emotion tag parser — extracts [emotion:xxx] tags from LLM responses."""

from __future__ import annotations

import re
from typing import Optional

from makima_common.logging import get_logger

logger = get_logger(__name__)

# Pattern to match [emotion:xxx] at the end of a message
_EMOTION_PATTERN = re.compile(r"\[emotion:([a-z_0-9]+)\]\s*$", re.IGNORECASE)

# Valid animation names
VALID_ANIMATIONS = {
    "idle",
    "smile",
    "think",
    "action",
    "expression_0",
    "no",
    "special",
    "talk_start",
    "talk_end",
}


def extract_emotion(text: str) -> tuple[str, Optional[str]]:
    """Extract emotion tag from the end of a message.

    Looks for `[emotion:xxx]` at the end of the text, removes it from the
    content, and returns the cleaned text and the animation name.

    Args:
        text: The raw LLM response text.

    Returns:
        A tuple of (cleaned_text, animation_name).
        animation_name is None if no valid emotion tag was found.
    """
    if not text:
        return text, None

    match = _EMOTION_PATTERN.search(text)
    if not match:
        return text, None

    animation_name = match.group(1).lower()

    # Validate against known animations
    if animation_name not in VALID_ANIMATIONS:
        logger.warning(
            "Unknown emotion tag, defaulting to idle",
            requested=animation_name,
            valid=list(VALID_ANIMATIONS),
        )
        animation_name = "idle"

    # Remove the emotion tag from the text
    cleaned = text[: match.start()].rstrip()

    logger.debug(
        "Extracted emotion tag",
        animation=animation_name,
        original_length=len(text),
        cleaned_length=len(cleaned),
    )

    return cleaned, animation_name