"""API request/response DTO definitions."""

from __future__ import annotations

from datetime import datetime
from typing import Any, Literal
from uuid import UUID

from pydantic import BaseModel, Field


# ── Auth ─────────────────────────────────────────────────────────────


class UserCreate(BaseModel):
    """Request body for user registration."""

    username: str = Field(..., min_length=3, max_length=50, description="用户名")
    email: str = Field(..., description="邮箱地址")
    password: str = Field(..., min_length=6, description="密码")


class UserLogin(BaseModel):
    """Request body for user login."""

    username: str = Field(..., description="用户名")
    password: str = Field(..., description="密码")


class TokenResponse(BaseModel):
    """JWT token response."""

    access_token: str = Field(..., description="JWT access token")
    token_type: str = Field(default="bearer", description="Token type")
    user_id: str = Field(..., description="User UUID")


# ── Session ──────────────────────────────────────────────────────────


class SessionCreate(BaseModel):
    """Request body for creating a new session."""

    title: str = Field(default="New Chat", description="会话标题")


class SessionResponse(BaseModel):
    """Single session response."""

    id: UUID = Field(..., description="会话 ID")
    user_id: UUID = Field(..., description="所属用户 ID")
    title: str = Field(..., description="会话标题")
    status: Literal["active", "closed"] = Field(..., description="会话状态")
    created_at: datetime = Field(..., description="创建时间")
    updated_at: datetime = Field(..., description="更新时间")


class SessionList(BaseModel):
    """Paginated session list."""

    items: list[SessionResponse] = Field(default_factory=list, description="会话列表")
    total: int = Field(default=0, description="总数")


# ── Message ──────────────────────────────────────────────────────────


class MessageCreate(BaseModel):
    """Request body for sending a message."""

    role: Literal["user", "system"] = Field(default="user", description="消息角色")
    content: str = Field(..., min_length=1, description="消息内容")


class MessageResponse(BaseModel):
    """Single message response."""

    id: UUID = Field(..., description="消息 ID")
    session_id: UUID = Field(..., description="所属会话 ID")
    role: Literal["user", "assistant", "system", "tool"] = Field(..., description="消息角色")
    content: str = Field(..., description="消息内容")
    metadata_: dict[str, Any] = Field(default_factory=dict, description="附加元数据")
    created_at: datetime = Field(..., description="创建时间")


# ── Task ─────────────────────────────────────────────────────────────


class ModelOverride(BaseModel):
    """Optional model configuration override from the client."""

    model: str | None = Field(default=None, description="Model identifier override")
    api_key: str | None = Field(default=None, description="API key override")
    base_url: str | None = Field(default=None, description="API base URL override")
    temperature: float | None = Field(default=None, ge=0.0, le=2.0, description="Temperature override")


class AttachmentInfo(BaseModel):
    """Metadata for an uploaded attachment, sent alongside a task request."""

    attachment_id: str = Field(..., description="Unique attachment identifier")
    original_name: str = Field(..., description="Original filename")
    mime_type: str = Field(..., description="MIME type of the file")
    stored_path: str = Field(..., description="Relative path under tool_working_dir")
    is_text: bool = Field(..., description="Whether the file is text (safe to inline)")
    size: int = Field(..., description="File size in bytes")


class TaskCreate(BaseModel):
    """Request body for creating an agent task."""

    session_id: UUID = Field(..., description="所属会话 ID")
    input_text: str = Field(..., min_length=1, description="用户输入文本")
    mode_slug: str | None = Field(default=None, description="Mode slug override")
    model_override: ModelOverride | None = Field(default=None, description="Model config override from client")
    attachments: list[AttachmentInfo] = Field(default_factory=list, description="Uploaded file attachments")


class TaskResponse(BaseModel):
    """Agent task response."""

    id: UUID = Field(..., description="任务 ID")
    session_id: UUID = Field(..., description="所属会话 ID")
    status: Literal["pending", "running", "completed", "failed"] = Field(
        ..., description="任务状态"
    )
    input_text: str = Field(..., description="用户输入")
    result: str | None = Field(default=None, description="Agent 最终输出")
    step_count: int = Field(default=0, description="执行步骤数")
    error: str | None = Field(default=None, description="错误信息")
    created_at: datetime = Field(..., description="创建时间")
    updated_at: datetime = Field(..., description="更新时间")


# ── Health ───────────────────────────────────────────────────────────


class HealthResponse(BaseModel):
    """Health check response."""

    status: str = Field(default="ok", description="服务状态")
    version: str = Field(..., description="应用版本号")