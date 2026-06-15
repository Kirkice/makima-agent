"""Prometheus metrics for monitoring and alerting."""

from __future__ import annotations

from prometheus_client import Counter, Histogram, Gauge, generate_latest, CONTENT_TYPE_LATEST
from fastapi import APIRouter, Response

router = APIRouter(tags=["metrics"])

# ── HTTP Metrics ─────────────────────────────────────────────────────
HTTP_REQUESTS_TOTAL = Counter(
    "http_requests_total",
    "Total HTTP requests",
    ["method", "endpoint", "status_code"],
)

HTTP_REQUEST_DURATION = Histogram(
    "http_request_duration_seconds",
    "HTTP request duration in seconds",
    ["method", "endpoint"],
    buckets=(0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0),
)

HTTP_REQUESTS_IN_PROGRESS = Gauge(
    "http_requests_in_progress",
    "Number of HTTP requests currently in progress",
    ["method"],
)

# ── Agent Metrics ────────────────────────────────────────────────────
AGENT_TASKS_TOTAL = Counter(
    "agent_tasks_total",
    "Total agent tasks executed",
    ["status"],
)

AGENT_TASK_DURATION = Histogram(
    "agent_task_duration_seconds",
    "Agent task execution duration",
    buckets=(1.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0),
)

AGENT_STEPS_TOTAL = Counter(
    "agent_steps_total",
    "Total agent execution steps",
    ["step_type"],
)

# ── Tool Metrics ─────────────────────────────────────────────────────
TOOL_CALLS_TOTAL = Counter(
    "tool_calls_total",
    "Total tool invocations",
    ["tool_name", "status"],
)

TOOL_CALL_DURATION = Histogram(
    "tool_call_duration_seconds",
    "Tool execution duration",
    ["tool_name"],
    buckets=(0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0),
)

# ── Memory Metrics ───────────────────────────────────────────────────
MEMORY_OPERATIONS_TOTAL = Counter(
    "memory_operations_total",
    "Total memory operations",
    ["operation"],
)

MEMORY_ITEMS_STORED = Gauge(
    "memory_items_stored",
    "Number of memory items stored",
)

# ── Knowledge Metrics ────────────────────────────────────────────────
KNOWLEDGE_DOCUMENTS_TOTAL = Gauge(
    "knowledge_documents_total",
    "Total documents in knowledge base",
)

KNOWLEDGE_CHUNKS_TOTAL = Gauge(
    "knowledge_chunks_total",
    "Total document chunks",
)

KNOWLEDGE_RETRIEVAL_DURATION = Histogram(
    "knowledge_retrieval_duration_seconds",
    "Knowledge retrieval duration",
    buckets=(0.01, 0.05, 0.1, 0.5, 1.0, 2.0),
)

# ── Queue Metrics ────────────────────────────────────────────────────
QUEUE_TASKS_TOTAL = Counter(
    "queue_tasks_total",
    "Total queued tasks",
    ["task_name", "status"],
)

QUEUE_TASK_DURATION = Histogram(
    "queue_task_duration_seconds",
    "Queued task execution duration",
    ["task_name"],
)


@router.get("/metrics")
async def metrics_endpoint() -> Response:
    """Expose Prometheus metrics."""
    return Response(
        content=generate_latest(),
        media_type=CONTENT_TYPE_LATEST,
    )


def setup_metrics(app) -> None:
    """Register metrics router on the app."""
    app.include_router(router)