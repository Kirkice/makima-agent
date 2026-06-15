"""Middleware for retry, timeout, and request processing."""

from __future__ import annotations

import asyncio
import time
import uuid
from typing import Any, Callable

from fastapi import FastAPI, Request, Response
from starlette.middleware.base import BaseHTTPMiddleware

from makima_common.logging import get_logger

logger = get_logger(__name__)


class RequestIDMiddleware(BaseHTTPMiddleware):
    """Add unique request ID to each request for tracing."""

    async def dispatch(self, request: Request, call_next: Callable) -> Response:
        request_id = str(uuid.uuid4())
        request.state.request_id = request_id

        start_time = time.time()
        response = await call_next(request)
        duration_ms = int((time.time() - start_time) * 1000)

        response.headers["X-Request-ID"] = request_id
        response.headers["X-Response-Time"] = f"{duration_ms}ms"

        logger.info(
            "Request processed",
            request_id=request_id,
            method=request.method,
            path=request.url.path,
            status=response.status_code,
            duration_ms=duration_ms,
        )

        return response


class TimeoutMiddleware(BaseHTTPMiddleware):
    """Apply timeout to requests."""

    def __init__(self, app: FastAPI, timeout_seconds: float = 30.0) -> None:
        super().__init__(app)
        self.timeout_seconds = timeout_seconds

    async def dispatch(self, request: Request, call_next: Callable) -> Response:
        try:
            response = await asyncio.wait_for(
                call_next(request),
                timeout=self.timeout_seconds,
            )
            return response
        except asyncio.TimeoutError:
            logger.warning(
                "Request timeout",
                path=request.url.path,
                timeout=self.timeout_seconds,
            )
            return Response(
                content='{"detail": "Request timeout"}',
                status_code=504,
                media_type="application/json",
            )


class RetryConfig:
    """Configuration for retry behavior."""

    def __init__(
        self,
        max_retries: int = 3,
        base_delay: float = 1.0,
        max_delay: float = 60.0,
        backoff_factor: float = 2.0,
        retryable_exceptions: tuple[type[Exception], ...] = (Exception,),
    ) -> None:
        self.max_retries = max_retries
        self.base_delay = base_delay
        self.max_delay = max_delay
        self.backoff_factor = backoff_factor
        self.retryable_exceptions = retryable_exceptions


async def retry_async(
    func: Callable,
    *args: Any,
    config: RetryConfig | None = None,
    **kwargs: Any,
) -> Any:
    """Execute an async function with retry logic."""
    if config is None:
        config = RetryConfig()

    last_exception: Exception | None = None
    delay = config.base_delay

    for attempt in range(config.max_retries + 1):
        try:
            return await func(*args, **kwargs)
        except config.retryable_exceptions as exc:
            last_exception = exc
            if attempt < config.max_retries:
                logger.warning(
                    "Retry attempt",
                    function=func.__name__,
                    attempt=attempt + 1,
                    delay=delay,
                    error=str(exc),
                )
                await asyncio.sleep(delay)
                delay = min(delay * config.backoff_factor, config.max_delay)

    raise last_exception  # type: ignore[misc]


def setup_middleware(app: FastAPI) -> None:
    """Register all middleware on the FastAPI app."""
    from makima_common.config import get_settings

    settings = get_settings()

    app.add_middleware(RequestIDMiddleware)
    app.add_middleware(TimeoutMiddleware, timeout_seconds=settings.api_timeout)