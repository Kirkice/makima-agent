"""Mode system definitions for multi-mode Agent."""

from __future__ import annotations

from enum import Enum
from typing import Literal

from pydantic import BaseModel, Field


class ToolGroup(str, Enum):
    """Tool group categories for permission control."""

    READ = "read"  # Read-only: file_read, search, list
    WRITE = "write"  # Write operations: file_write, edit
    COMMAND = "command"  # Command execution: shell
    NETWORK = "network"  # Network requests: http
    MCP = "mcp"  # MCP tools
    SYSTEM = "system"  # System tools: switch_mode


class ToolGroupConfig(BaseModel):
    """Configuration for a tool group within a mode."""

    group: ToolGroup = Field(..., description="Tool group identifier")
    file_regex: str | None = Field(default=None, description="File filter regex, e.g. '\\.md$'")
    auto_approve: bool = Field(default=True, description="Whether to auto-approve operations")


class ModeConfig(BaseModel):
    """Configuration for an Agent mode.

    Modes define different behavioral profiles for the Agent, including:
    - Role and personality (via role_definition)
    - Available tools (via tool_groups)
    - LLM parameters (temperature, max_steps)
    """

    slug: str = Field(..., description="Unique identifier, e.g. 'code', 'architect'")
    name: str = Field(..., description="Display name, e.g. '🛠️ Code'")
    role_definition: str = Field(..., description="Role definition injected into system prompt")
    when_to_use: str | None = Field(
        default=None, description="Description of when to use this mode (shown to LLM)"
    )
    description: str | None = Field(default=None, description="Short description for UI")
    custom_instructions: str | None = Field(
        default=None, description="Additional instructions appended to system prompt"
    )
    tool_groups: list[ToolGroupConfig] = Field(
        default_factory=list, description="Available tool groups"
    )
    max_steps: int = Field(default=30, ge=1, le=100, description="Maximum execution steps")
    temperature: float = Field(default=0.0, ge=0.0, le=2.0, description="LLM temperature")
    source: Literal["builtin", "project", "custom"] = Field(
        default="builtin", description="Mode source"
    )


class ModeSwitchRequest(BaseModel):
    """Request to switch the current mode."""

    mode_slug: str = Field(..., description="Target mode slug")
    reason: str | None = Field(default=None, description="Reason for switching")


class ModeSwitchResponse(BaseModel):
    """Response after mode switch."""

    previous_mode: str = Field(..., description="Previous mode slug")
    current_mode: str = Field(..., description="New mode slug")
    mode_name: str = Field(..., description="New mode display name")
    available_tools: list[str] = Field(default_factory=list, description="Available tool names")