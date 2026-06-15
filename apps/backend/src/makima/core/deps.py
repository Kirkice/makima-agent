"""FastAPI dependency injection helpers."""

from __future__ import annotations

from collections.abc import AsyncGenerator
from uuid import UUID

from fastapi import Depends, HTTPException, status
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from makima.auth.service import get_current_user_id
from makima.auth.models import User
from makima.core.db import get_async_session
from makima_common.config import Settings, get_settings


async def get_db() -> AsyncGenerator[AsyncSession, None]:
    """Alias for get_async_session."""
    async for session in get_async_session():
        yield session


async def get_current_user(
    user_id: str = Depends(get_current_user_id),
    db: AsyncSession = Depends(get_db),
) -> User:
    """Fetch the current user from the database."""
    result = await db.execute(select(User).where(User.id == UUID(user_id)))
    user = result.scalar_one_or_none()
    if user is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND, detail="User not found"
        )
    return user


def get_app_settings() -> Settings:
    """Return application settings."""
    return get_settings()