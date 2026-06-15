"""Memory API routes — manage user memories."""

from __future__ import annotations

from typing import Any

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel, Field

from makima.auth.models import User
from makima.core.deps import get_current_user
from makima.memory.service import MemoryService
from makima_common.logging import get_logger

logger = get_logger(__name__)

router = APIRouter(prefix="/memories", tags=["memory"])


class MemoryResponse(BaseModel):
    """Single memory entry."""

    id: str = Field(..., description="Memory ID")
    memory: str = Field(..., description="Memory content")
    score: float | None = Field(None, description="Relevance score (if from search)")
    metadata: dict[str, Any] = Field(default_factory=dict, description="Memory metadata")


class MemoryListResponse(BaseModel):
    """List of memories."""

    memories: list[MemoryResponse] = Field(default_factory=list)
    count: int = Field(..., description="Total count")


class MemorySearchRequest(BaseModel):
    """Search memories request."""

    query: str = Field(..., min_length=1, description="Search query")
    limit: int = Field(default=5, ge=1, le=20, description="Max results")


class MemoryStatusResponse(BaseModel):
    """Memory service status."""

    available: bool = Field(..., description="Whether memory service is available")
    provider: str = Field(default="mem0", description="Memory provider name")


@router.get("/status", response_model=MemoryStatusResponse)
async def get_memory_status(
    user: User = Depends(get_current_user),
) -> MemoryStatusResponse:
    """Check if memory service is available."""
    service = MemoryService()
    return MemoryStatusResponse(available=service.available)


@router.get("", response_model=MemoryListResponse)
async def list_memories(
    user: User = Depends(get_current_user),
) -> MemoryListResponse:
    """List all memories for the current user."""
    service = MemoryService()
    if not service.available:
        raise HTTPException(status_code=503, detail="Memory service unavailable")

    memories = service.get_user_memories(user_id=str(user.id))
    return MemoryListResponse(
        memories=[
            MemoryResponse(
                id=m.get("id", ""),
                memory=m.get("memory", ""),
                metadata=m.get("metadata", {}),
            )
            for m in memories
        ],
        count=len(memories),
    )


@router.post("/search", response_model=MemoryListResponse)
async def search_memories(
    request: MemorySearchRequest,
    user: User = Depends(get_current_user),
) -> MemoryListResponse:
    """Search memories by query."""
    service = MemoryService()
    if not service.available:
        raise HTTPException(status_code=503, detail="Memory service unavailable")

    memories = service.recall(query=request.query, user_id=str(user.id), limit=request.limit)
    return MemoryListResponse(
        memories=[
            MemoryResponse(
                id=m.get("id", ""),
                memory=m.get("memory", ""),
                score=m.get("score"),
                metadata=m.get("metadata", {}),
            )
            for m in memories
        ],
        count=len(memories),
    )


@router.delete("/{memory_id}")
async def delete_memory(
    memory_id: str,
    user: User = Depends(get_current_user),
) -> dict[str, str]:
    """Delete a specific memory."""
    service = MemoryService()
    if not service.available:
        raise HTTPException(status_code=503, detail="Memory service unavailable")

    success = service.delete_memory(memory_id)
    if not success:
        raise HTTPException(status_code=404, detail="Memory not found")

    return {"status": "deleted", "id": memory_id}