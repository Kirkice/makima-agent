"""Role definition section for system prompt."""

from makima_schemas import ModeConfig


def build_role_section(mode: ModeConfig) -> str:
    """Build the role definition section from mode config."""
    return f"# Your Role\n\n{mode.role_definition}"