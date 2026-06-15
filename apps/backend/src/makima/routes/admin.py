"""Admin API routes for system management."""

from __future__ import annotations

from typing import Any

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel, Field
from sqlalchemy.ext.asyncio import AsyncSession

from makima.auth.models import User
from makima.config_center.service import config_center
from makima.core.deps import get_current_user, get_db
from makima.security.rbac import Permission, check_permission

router = APIRouter(prefix="/admin", tags=["admin"])


class ConfigUpdateRequest(BaseModel):
    """Request model for updating configuration."""

    key: str = Field(..., description="Configuration key")
    value: Any = Field(..., description="Configuration value")
    ttl: int | None = Field(None, description="Optional TTL in seconds")


class ConfigResponse(BaseModel):
    """Response model for configuration."""

    key: str
    value: Any


@router.get("/config", response_model=list[ConfigResponse])
async def list_config(
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> list[ConfigResponse]:
    """List all configuration values. Requires admin permission."""
    if not check_permission(current_user, Permission.ADMIN_SYSTEM):
        raise HTTPException(status_code=403, detail="Insufficient permissions")

    config = await config_center.get_all()
    return [ConfigResponse(key=k, value=v) for k, v in config.items()]


@router.put("/config")
async def update_config(
    request: ConfigUpdateRequest,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> dict[str, Any]:
    """Update a configuration value. Requires admin permission."""
    if not check_permission(current_user, Permission.ADMIN_SYSTEM):
        raise HTTPException(status_code=403, detail="Insufficient permissions")

    success = await config_center.set(request.key, request.value, request.ttl)
    if not success:
        raise HTTPException(status_code=500, detail="Failed to update configuration")

    return {"status": "updated", "key": request.key}


@router.delete("/config/{key}")
async def delete_config(
    key: str,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> dict[str, Any]:
    """Delete a configuration value. Requires admin permission."""
    if not check_permission(current_user, Permission.ADMIN_SYSTEM):
        raise HTTPException(status_code=403, detail="Insufficient permissions")

    success = await config_center.delete(key)
    if not success:
        raise HTTPException(status_code=404, detail="Configuration key not found")

    return {"status": "deleted", "key": key}


@router.get("/health")
async def admin_health_check(
    current_user: User = Depends(get_current_user),
) -> dict[str, Any]:
    """Admin health check with detailed system info."""
    if not check_permission(current_user, Permission.ADMIN_SYSTEM):
        raise HTTPException(status_code=403, detail="Insufficient permissions")

    redis_available = config_center._redis is not None

    return {
        "status": "healthy" if redis_available else "degraded",
        "config_center": "connected" if redis_available else "disconnected",
        "cache_size": len(config_center._cache),
    }