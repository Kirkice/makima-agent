"""Backward-compatible re-exports from core.db.

DEPRECATED: Import from ``makima.core.db`` directly.
"""

from __future__ import annotations

from makima.core.db import engine, get_async_session
from makima.core.models import Base

__all__ = ["Base", "engine", "get_async_session"]