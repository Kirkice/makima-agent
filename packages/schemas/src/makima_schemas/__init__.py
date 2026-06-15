"""Makima Schemas — Event protocol and DTO definitions."""

from __future__ import annotations

from makima_schemas.api import (
    HealthResponse,
    MessageCreate,
    MessageResponse,
    SessionCreate,
    SessionList,
    SessionResponse,
    TaskCreate,
    TaskResponse,
    TokenResponse,
    UserCreate,
    UserLogin,
)
from makima_schemas.events import AgentEvent, AgentEventType
from makima_schemas.tools import (
    ToolCallRequest,
    ToolCallResult,
    ToolDefinition,
    ToolParameter,
)

__all__ = [
    # Events
    "AgentEvent",
    "AgentEventType",
    # API
    "HealthResponse",
    "MessageCreate",
    "MessageResponse",
    "SessionCreate",
    "SessionList",
    "SessionResponse",
    "TaskCreate",
    "TaskResponse",
    "TokenResponse",
    "UserCreate",
    "UserLogin",
    # Tools
    "ToolCallRequest",
    "ToolCallResult",
    "ToolDefinition",
    "ToolParameter",
]