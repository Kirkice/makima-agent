"""Switch mode tool — allows LLM to request mode changes."""

from __future__ import annotations

from typing import TYPE_CHECKING

from langchain_core.tools import tool

from makima_common.logging import get_logger

if TYPE_CHECKING:
    from makima.modes.registry import ModeRegistry

logger = get_logger(__name__)


@tool
def switch_mode(mode_slug: str, reason: str = "") -> str:
    """Request to switch to a different agent mode.

    Use this tool when the current task would benefit from a different mode's capabilities.
    For example, switch to 'architect' for system design, 'debug' for troubleshooting,
    or 'chat' for casual conversation.

    Args:
        mode_slug: The slug identifier of the target mode (e.g., 'code', 'architect', 'ask', 'debug', 'chat', 'companion')
        reason: Brief explanation of why you want to switch modes

    Returns:
        A message indicating the mode switch was requested
    """
    logger.info(
        "Mode switch requested",
        target_mode=mode_slug,
        reason=reason,
    )

    return (
        f"Mode switch to '{mode_slug}' has been requested. "
        f"Reason: {reason or 'No reason provided'}. "
        f"The system will switch to the new mode for the next response."
    )