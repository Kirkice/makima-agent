"""Session management routes."""

from __future__ import annotations

from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from makima.auth.models import User
from makima.core.deps import get_current_user, get_db
from makima.sessions.models import Session
from makima_schemas.api import SessionCreate, SessionList, SessionResponse
from pydantic import BaseModel

router = APIRouter(prefix="/sessions", tags=["sessions"])


@router.post("", response_model=SessionResponse, status_code=status.HTTP_201_CREATED)
async def create_session(
    body: SessionCreate,
    user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> SessionResponse:
    """Create a new chat session."""
    session = Session(user_id=user.id, title=body.title)
    db.add(session)
    await db.flush()
    return SessionResponse(
        id=session.id,
        user_id=session.user_id,
        title=session.title,
        status=session.status,
        created_at=session.created_at,
        updated_at=session.updated_at,
    )


@router.get("", response_model=SessionList)
async def list_sessions(
    user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> SessionList:
    """List all sessions for the current user."""
    result = await db.execute(
        select(Session).where(Session.user_id == user.id).order_by(Session.updated_at.desc())
    )
    sessions = result.scalars().all()
    items = [
        SessionResponse(
            id=s.id,
            user_id=s.user_id,
            title=s.title,
            status=s.status,
            created_at=s.created_at,
            updated_at=s.updated_at,
        )
        for s in sessions
    ]
    return SessionList(items=items, total=len(items))


@router.get("/{session_id}", response_model=SessionResponse)
async def get_session(
    session_id: UUID,
    user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> SessionResponse:
    """Get a single session by ID."""
    result = await db.execute(
        select(Session).where(Session.id == session_id, Session.user_id == user.id)
    )
    session = result.scalar_one_or_none()
    if session is None:
        raise HTTPException(status_code=404, detail="Session not found")
    return SessionResponse(
        id=session.id,
        user_id=session.user_id,
        title=session.title,
        status=session.status,
        created_at=session.created_at,
        updated_at=session.updated_at,
    )


class SessionUpdate(BaseModel):
    """Request body for updating a session."""
    title: str | None = None


@router.patch("/{session_id}", response_model=SessionResponse)
async def update_session(
    session_id: UUID,
    body: SessionUpdate,
    user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> SessionResponse:
    """Update session title."""
    result = await db.execute(
        select(Session).where(Session.id == session_id, Session.user_id == user.id)
    )
    session = result.scalar_one_or_none()
    if session is None:
        raise HTTPException(status_code=404, detail="Session not found")
    
    if body.title is not None:
        session.title = body.title
    
    await db.flush()
    return SessionResponse(
        id=session.id,
        user_id=session.user_id,
        title=session.title,
        status=session.status,
        created_at=session.created_at,
        updated_at=session.updated_at,
    )


@router.delete("/{session_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_session(
    session_id: UUID,
    user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> None:
    """Delete a session."""
    result = await db.execute(
        select(Session).where(Session.id == session_id, Session.user_id == user.id)
    )
    session = result.scalar_one_or_none()
    if session is None:
        raise HTTPException(status_code=404, detail="Session not found")
    await db.delete(session)