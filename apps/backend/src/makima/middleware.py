"""Backward-compatible re-exports from core.middleware.

DEPRECATED: Import from `makima.core.middleware` directly.
"""

from __future__ import annotations

from makima.core.middleware import (
    RequestIDMiddleware,
    RetryConfig,
    TimeoutMiddleware,
    retry_async,
    setup_middleware,
)

__all__ = [
    "RequestIDMiddleware",
    "RetryConfig",
    "TimeoutMiddleware",
    "retry_async",
    "setup_middleware",
]
