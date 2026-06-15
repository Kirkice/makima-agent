"""Audit log API routes."""

from __future__ import annotations

from datetime import datetime
from typing import Any
from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException, Query
from pydantic import BaseModel
from sqlalchemy.ext.asyncio import AsyncSession

from makima.audit.models import AuditLog
from makima.audit.service import AuditService
from makima.auth.models import User
from makima.core.deps import get_current_user, get_db

router = APIRouter(prefix="/audit", tags=["audit"])


class AuditLogResponse(BaseModel):
    """Audit log response model."""

    id: str
    user_id: str | None
    user_email: str | None
    user_role: str | None
    action: str
    severity: str
    resource_type: str | None
    resource_id: str | None
    ip_address: str | None
    request_id: str | None
    details: dict[str, Any] | None
    error_message: str | None
    timestamp: str
    duration_ms: int | None


class AuditLogListResponse(BaseModel):
    """List of audit logs."""

    items: list[AuditLogResponse]
    total: int


def _log_to_response(log: AuditLog) -> AuditLogResponse:
    return AuditLogResponse(
        id=str(log.id),
        user_id=str(log.user_id) if log.user_id else None,
        user_email=log.user_email,
        user_role=log.user_role,
        action=log.action,
        severity=log.severity,
        resource_type=log.resource_type,
        resource_id=log.resource_id,
        ip_address=log.ip_address,
        request_id=log.request_id,
        details=log.details,
        error_message=log.error_message,
        timestamp=log.timestamp.isoformat(),
        duration_ms=log.duration_ms,
    )


@router.get("", response_model=AuditLogListResponse)
async def list_audit_logs(
    user_id: UUID | None = Query(None),
    action: str | None = Query(None),
    severity: str | None = Query(None),
    resource_type: str | None = Query(None),
    limit: int = Query(100, ge=1, le=1000),
    offset: int = Query(0, ge=0),
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> AuditLogListResponse:
    """Query audit logs. Requires admin role."""
    user_role = getattr(current_user, "role", "user")
    if user_role != "admin":
        raise HTTPException(status_code=403, detail="Admin role required")

    logs = await AuditService.query_logs(
        db=db,
        user_id=user_id,
        action=action,
        severity=severity,
        resource_type=resource_type,
        limit=limit,
        offset=offset,
    )

    return AuditLogListResponse(
        items=[_log_to_response(log) for log in logs],
        total=len(logs),
    )