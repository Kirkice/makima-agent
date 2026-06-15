"""Audit logging service."""

from __future__ import annotations

import traceback
import uuid
from typing import Any

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from makima.audit.models import AuditAction, AuditLog, AuditSeverity
from makima_common.logging import get_logger

logger = get_logger(__name__)


class AuditService:
    """Service for recording audit events."""

    @staticmethod
    async def log(
        db: AsyncSession,
        action: AuditAction,
        *,
        user_id: uuid.UUID | None = None,
        user_email: str | None = None,
        user_role: str | None = None,
        severity: AuditSeverity = AuditSeverity.INFO,
        resource_type: str | None = None,
        resource_id: str | None = None,
        ip_address: str | None = None,
        user_agent: str | None = None,
        request_id: str | None = None,
        details: dict[str, Any] | None = None,
        error_message: str | None = None,
        error_stack: str | None = None,
        duration_ms: int | None = None,
    ) -> AuditLog:
        """Record an audit event."""
        entry = AuditLog(
            action=action.value,
            user_id=user_id,
            user_email=user_email,
            user_role=user_role,
            severity=severity.value,
            resource_type=resource_type,
            resource_id=resource_id,
            ip_address=ip_address,
            user_agent=user_agent,
            request_id=request_id,
            details=details,
            error_message=error_message,
            error_stack=error_stack,
            duration_ms=duration_ms,
        )
        db.add(entry)
        await db.flush()

        logger.info(
            "Audit event recorded",
            action=action.value,
            user_id=str(user_id) if user_id else None,
            severity=severity.value,
        )
        return entry

    @staticmethod
    async def log_exception(
        db: AsyncSession,
        action: AuditAction,
        exception: Exception,
        **kwargs: Any,
    ) -> AuditLog:
        """Record an audit event for an exception."""
        return await AuditService.log(
            db=db,
            action=action,
            severity=AuditSeverity.ERROR,
            error_message=str(exception),
            error_stack=traceback.format_exc(),
            **kwargs,
        )

    @staticmethod
    async def query_logs(
        db: AsyncSession,
        *,
        user_id: uuid.UUID | None = None,
        action: str | None = None,
        severity: str | None = None,
        resource_type: str | None = None,
        limit: int = 100,
        offset: int = 0,
    ) -> list[AuditLog]:
        """Query audit logs with filters."""
        stmt = select(AuditLog)

        if user_id:
            stmt = stmt.where(AuditLog.user_id == user_id)
        if action:
            stmt = stmt.where(AuditLog.action == action)
        if severity:
            stmt = stmt.where(AuditLog.severity == severity)
        if resource_type:
            stmt = stmt.where(AuditLog.resource_type == resource_type)

        stmt = stmt.order_by(AuditLog.timestamp.desc()).offset(offset).limit(limit)

        result = await db.execute(stmt)
        return list(result.scalars().all())