"""Agent task routes with SSE streaming."""

from __future__ import annotations

import time
from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession
from sse_starlette.sse import EventSourceResponse
from langchain_core.messages import AIMessage, BaseMessage, HumanMessage, SystemMessage

from makima.auth.models import User
from makima.core.deps import get_current_user, get_db
from makima.orchestrator.runner import run_agent
from makima.sessions.models import Message, Session
from makima.tasks.models import Task
from makima_schemas.api import TaskCreate, TaskResponse
from makima_schemas.events import AgentEvent, AgentEventType

router = APIRouter(prefix="/tasks", tags=["tasks"])


def _to_langchain_message(message: Message) -> BaseMessage | None:
    if message.role == "user":
        return HumanMessage(content=message.content)
    if message.role == "assistant":
        return AIMessage(content=message.content)
    if message.role == "system":
        return SystemMessage(content=message.content)
    return None


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

    history_result = await db.execute(
        select(Message)
        .where(Session.id == body.session_id, Session.user_id == user.id)
        .join(Session, Session.id == Message.session_id)
        .order_by(Message.created_at.asc())
    )
    history_messages = [
        lc_msg
        for lc_msg in (_to_langchain_message(msg) for msg in history_result.scalars().all())
        if lc_msg is not None
    ]

    user_msg = Message(session_id=body.session_id, role="user", content=body.input_text)
    db.add(user_msg)

    task = Task(session_id=body.session_id, input_text=body.input_text, status="running")
    db.add(task)
    await db.flush()
    task_id = str(task.id)
    await db.commit()

    # Extract model override from request body
    model_override = None
    if body.model_override:
        model_override = body.model_override.model_dump(exclude_none=True)

    # Use mode_slug from request or default
    mode_slug = body.mode_slug or "code"

    async def event_generator():
        last_assistant_message: str | None = None
        step_count = 0
        try:
            async for event in run_agent(
                input_text=body.input_text,
                session_id=str(body.session_id),
                user_id=str(user.id),
                mode_slug=mode_slug,
                db=db,
                model_override=model_override,
                attachments=body.attachments if body.attachments else None,
                history_messages=history_messages,
            ):
                step_count = max(step_count, event.step)
                if event.type == AgentEventType.MESSAGE:
                    content = event.data.get("content")
                    if isinstance(content, str) and content.strip():
                        last_assistant_message = content
                yield {"event": event.type.value, "data": event.model_dump_json()}

            if last_assistant_message:
                db.add(
                    Message(
                        session_id=body.session_id,
                        role="assistant",
                        content=last_assistant_message,
                    )
                )
            task_db = await db.get(Task, UUID(task_id))
            if task_db is not None:
                task_db.status = "completed"
                task_db.result = last_assistant_message
                task_db.step_count = step_count
                task_db.error = None
            await db.commit()
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
            task_db = await db.get(Task, UUID(task_id))
            if task_db is not None:
                task_db.status = "failed"
                task_db.error = str(e)
                task_db.step_count = step_count
            await db.commit()
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
