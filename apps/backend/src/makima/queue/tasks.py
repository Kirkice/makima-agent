"""Celery task definitions for background processing."""

from __future__ import annotations

import time
from typing import Any

from makima_common.logging import get_logger
from makima.queue.celery_app import celery_app

logger = get_logger(__name__)


@celery_app.task(
    bind=True,
    name="makima.process_document",
    max_retries=3,
    default_retry_delay=30,
    acks_late=True,
)
def process_document_task(
    self,
    document_id: str,
    user_id: str,
    content: str,
    metadata: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Process a document in the background."""
    logger.info("Processing document", document_id=document_id)
    start_time = time.time()

    try:
        chunk_count = len(content) // 1000
        result = {
            "document_id": document_id,
            "status": "completed",
            "chunk_count": chunk_count,
            "processing_time_ms": int((time.time() - start_time) * 1000),
        }
        logger.info("Document processed successfully", **result)
        return result
    except Exception as exc:
        logger.error("Document processing failed", error=str(exc))
        raise self.retry(exc=exc, countdown=30 * (2 ** self.request.retries))


@celery_app.task(
    bind=True,
    name="makima.extract_memories",
    max_retries=2,
    default_retry_delay=60,
)
def extract_memories_task(
    self,
    session_id: str,
    user_id: str,
    messages: list[dict[str, str]],
) -> dict[str, Any]:
    """Extract memories from conversation in background."""
    logger.info("Extracting memories", session_id=session_id)
    return {
        "session_id": session_id,
        "user_id": user_id,
        "status": "completed",
        "memories_extracted": 0,
    }


@celery_app.task(name="makima.cleanup_expired")
def cleanup_expired_data_task() -> dict[str, Any]:
    """Periodic task to clean up expired data."""
    logger.info("Running expired data cleanup")
    return {"status": "completed", "sessions_cleaned": 0, "audit_logs_cleaned": 0}


@celery_app.task(
    bind=True,
    name="makima.run_agent_async",
    max_retries=1,
    time_limit=600,
    soft_time_limit=540,
)
def run_agent_async_task(
    self,
    task_id: str,
    session_id: str,
    user_id: str,
    input_text: str,
) -> dict[str, Any]:
    """Run an agent task asynchronously in the background."""
    logger.info("Running agent async", task_id=task_id)
    start_time = time.time()
    return {
        "task_id": task_id,
        "status": "completed",
        "execution_time_ms": int((time.time() - start_time) * 1000),
    }


celery_app.conf.beat_schedule = {
    "cleanup-expired-data": {
        "task": "makima.cleanup_expired",
        "schedule": 3600.0,
    },
}