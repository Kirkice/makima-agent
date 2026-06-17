"""Agent event types for SSE streaming output."""

from __future__ import annotations

from enum import Enum
from typing import Any

from pydantic import BaseModel, Field


class AgentEventType(str, Enum):
    """Agent 事件类型枚举，用于 SSE 流式输出。"""

    THINKING = "thinking"
    TOOL_CALL = "tool_call"
    TOOL_RESULT = "tool_result"
    MESSAGE = "message"
    ERROR = "error"
    DONE = "done"
    MODE_SWITCH = "mode_switch"
    CHECKPOINT_SAVED = "checkpoint_saved"
    CHECKPOINT_RESTORED = "checkpoint_restored"
    APPROVAL_REQUESTED = "approval_requested"
    APPROVAL_RESPONDED = "approval_responded"
    CONTEXT_COMPRESSED = "context_compressed"
    RETRY_DELAYED = "retry_delayed"


class AgentEvent(BaseModel):
    """Agent 事件，用于 SSE 流式输出。"""

    type: AgentEventType = Field(..., description="事件类型")
    data: dict[str, Any] = Field(default_factory=dict, description="事件载荷数据")
    timestamp: float = Field(..., description="事件时间戳（Unix epoch）")
    step: int = Field(..., ge=0, description="当前执行步骤编号")