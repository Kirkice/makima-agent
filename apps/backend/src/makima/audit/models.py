"""Audit log database models."""

from __future__ import annotations

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any

from sqlalchemy import JSON, DateTime, ForeignKey, Integer, String, Text, Index
from sqlalchemy.dialects.postgresql import JSONB
from sqlalchemy.orm import Mapped, mapped_column

from makima.core.models import Base


class AuditAction(str, Enum):
    """Types of auditable actions."""

    # Authentication
    LOGIN = "auth:login"
    LOGOUT = "auth:logout"
    LOGIN_FAILED = "auth:login_failed"

    # Session operations
    SESSION_CREATE = "session:create"
    SESSION_DELETE = "session:delete"

    # Task operations
    TASK_CREATE = "task:create"
    TASK_EXECUTE = "task:execute"
    TASK_COMPLETE = "task:complete"
    TASK_FAILED = "task:failed"

    # Document operations
    DOCUMENT_UPLOAD = "document:upload"
    DOCUMENT_DELETE = "document:delete"
    DOCUMENT_PROCESS = "document:process"

    # Memory operations
    MEMORY_CREATE = "memory:create"
    MEMORY_DELETE = "memory:delete"

    # Tool operations
    TOOL_EXECUTE = "tool:execute"
    TOOL_FAILED = "tool:failed"

    # Admin operations
    ADMIN_USER_CREATE = "admin:user_create"
    ADMIN_USER_UPDATE = "admin:user_update"
    ADMIN_USER_DELETE = "admin:user_delete"
    ADMIN_CONFIG_UPDATE = "admin:config_update"

    # System
    SYSTEM_ERROR = "system:error"
    SYSTEM_WARNING = "system:warning"


class AuditSeverity(str, Enum):
    """Severity levels for audit events."""

    INFO = "info"
    WARNING = "warning"
    ERROR = "error"
    CRITICAL = "critical"


class AuditLog(Base):
    """Audit log entry for tracking user actions and system events."""

    __tablename__ = "audit_logs"
    __table_args__ = (
        Index("idx_audit_user_id", "user_id"),
        Index("idx_audit_action", "action"),
        Index("idx_audit_timestamp", "timestamp"),
        Index("idx_audit_resource_type", "resource_type"),
        Index("idx_audit_resource_id", "resource_id"),
    )

    id: Mapped[uuid.UUID] = mapped_column(primary_key=True, default=uuid.uuid4)
    
    # User information
    user_id: Mapped[uuid.UUID | None] = mapped_column(
        ForeignKey("users.id", ondelete="SET NULL"), nullable=True, index=True
    )
    user_email: Mapped[str | None] = mapped_column(String(255), nullable=True)
    user_role: Mapped[str | None] = mapped_column(String(50), nullable=True)
    
    # Action details
    action: Mapped[str] = mapped_column(String(100), nullable=False, index=True)
    severity: Mapped[str] = mapped_column(
        String(20), default=AuditSeverity.INFO.value, nullable=False
    )
    
    # Resource information
    resource_type: Mapped[str | None] = mapped_column(String(100), nullable=True, index=True)
    resource_id: Mapped[str | None] = mapped_column(String(255), nullable=True, index=True)
    
    # Request context
    ip_address: Mapped[str | None] = mapped_column(String(45), nullable=True)
    user_agent: Mapped[str | None] = mapped_column(String(500), nullable=True)
    request_id: Mapped[str | None] = mapped_column(String(100), nullable=True)
    
    # Additional metadata
    details: Mapped[dict[str, Any] | None] = mapped_column(JSONB, nullable=True)
    error_message: Mapped[str | None] = mapped_column(Text, nullable=True)
    error_stack: Mapped[str | None] = mapped_column(Text, nullable=True)
    
    # Timing
    timestamp: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), default=lambda: datetime.now(timezone.utc), nullable=False
    )
    duration_ms: Mapped[int | None] = mapped_column(Integer, nullable=True)

    def to_dict(self) -> dict[str, Any]:
        """Convert audit log to dictionary."""
        return {
            "id": str(self.id),
            "user_id": str(self.user_id) if self.user_id else None,
            "user_email": self.user_email,
            "user_role": self.user_role,
            "action": self.action,
            "severity": self.severity,
            "resource_type": self.resource_type,
            "resource_id": self.resource_id,
            "ip_address": self.ip_address,
            "user_agent": self.user_agent,
            "request_id": self.request_id,
            "details": self.details,
            "error_message": self.error_message,
            "error_stack": self.error_stack,
            "timestamp": self.timestamp.isoformat(),
            "duration_ms": self.duration_ms,
        }