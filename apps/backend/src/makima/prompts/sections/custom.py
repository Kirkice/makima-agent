"""Custom instructions section for system prompt."""

from makima_schemas import ModeConfig


def build_custom_section(mode: ModeConfig) -> str:
    """Build the custom instructions section if present."""
    if not mode.custom_instructions:
        return ""

    return f"# Additional Instructions\n\n{mode.custom_instructions}"