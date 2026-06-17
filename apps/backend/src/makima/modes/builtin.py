"""Built-in mode definitions — minimal fallback.

The primary mode definitions now live in `.makima/modes.yaml`.
This file provides a single fallback mode in case the YAML config
is missing or fails to load.
"""

from makima_schemas import ModeConfig, ToolGroup, ToolGroupConfig

# Fallback: a minimal "code" mode if no YAML config is available
BUILTIN_MODES: list[ModeConfig] = [
    ModeConfig(
        slug="code",
        name="🛠️ Code",
        role_definition="You are Makima, a helpful AI assistant with access to various tools for file operations, shell commands, and HTTP requests.",
        when_to_use="Default mode for general tasks.",
        description="Default fallback mode with full tool access",
        tool_groups=[
            ToolGroupConfig(group=ToolGroup.READ),
            ToolGroupConfig(group=ToolGroup.WRITE),
            ToolGroupConfig(group=ToolGroup.COMMAND),
            ToolGroupConfig(group=ToolGroup.NETWORK),
            ToolGroupConfig(group=ToolGroup.SYSTEM),
        ],
        max_steps=50,
        temperature=0.0,
        source="builtin",
    ),
]