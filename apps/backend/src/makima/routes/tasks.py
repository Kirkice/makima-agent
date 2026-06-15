"""Agent task routes with SSE streaming."""

from __future__ import annotations

import time
from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession
from sse_starlette.sse import EventSourceResponse

from makima.deps import get_current_user, get_db
from makima.models import Message, Session, Task, User
from makima.orchestrator.runner import run_agent
from makima_schemas.api import TaskCreate, TaskResponse
from makima_schemas.events import AgentEvent, AgentEventType

router = APIRouter(prefix="/tasks", tags=["tasks"])


@router.post("")
async def create_task(
    body: TaskCreate,
    user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> EventSourceResponse:
    """Create an agent task and stream events via SSE."""
    result = await db.execute(
        select(Session).where(Session.id == body.session_id, Session.user_id == user.id)
    )
    if result.scalar_one_or_none() is None:
        raise HTTPException(status_code=404, detail="Session not found")

    user_msg = Message(session_id=body.session_id, role="user", content=body.input_text)
    db.add(user_msg)

    task = Task(session_id=body.session_id, input_text=body.input_text, status="running")
    db.add(task)
    await db.flush()
    task_id = str(task.id)

    async def event_generator():
        try:
            async for event in run_agent(
                input_text=body.input_text,
                session_id=str(body.session_id),
                task_id=task_id,
            ):
                yield {"event": event.type.value, "data": event.model_dump_json()}
            yield {
                "event": AgentEventType.DONE.value,
                "data": AgentEvent(
                    type=AgentEventType.DONE,
                    data={"task_id": task_id},
                    timestamp=time.time(),
                    step=0,
                ).model_dump_json(),
            }
        except Exception as e:
            yield {
                "event": AgentEventType.ERROR.value,
                "data": AgentEvent(
                    type=AgentEventType.ERROR,
                    data={"error": str(e)},
                    timestamp=time.time(),
                    step=0,
                ).model_dump_json(),
            }

    return EventSourceResponse(event_generator())


@router.get("/{task_id}", response_model=TaskResponse)
async def get_task(
    task_id: UUID,
    user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> TaskResponse:
    """Get task status and result."""
    result = await db.execute(select(Task).where(Task.id == task_id))
    task = result.scalar_one_or_none()
    if task is None:
        raise HTTPException(status_code=404, detail="Task not found")
    return TaskResponse(
        id=task.id,
        session_id=task.session_id,
        status=task.status,
        input_text=task.input_text,
        result=task.result,
        step_count=task.step_count,
        error=task.error,
        created_at=task.created_at,
        updated_at=task.updated_at,
    )