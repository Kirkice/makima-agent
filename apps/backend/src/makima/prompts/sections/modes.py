"""Modes section for system prompt."""

from makima_schemas import ModeConfig


def build_modes_section(modes: list[ModeConfig]) -> str:
    """Build the available modes section."""
    if not modes:
        return ""

    lines = [
        "# Available Modes\n",
        "You can switch between different modes based on the task. Use the `switch_mode` tool to request a mode change.\n",
    ]

    for mode in modes:
        when_to_use = mode.when_to_use or mode.description or ""
        lines.append(f"## {mode.name} (`{mode.slug}`)")
        if when_to_use:
            lines.append(f"{when_to_use}")
        lines.append("")

    lines.append("To switch modes, use: `switch_mode(mode_slug=\"<slug>\", reason=\"<why>\")`")

    return "\n".join(lines)