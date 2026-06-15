"""Backward-compatible re-exports from core.deps.

DEPRECATED: Import from ``makima.core.deps`` directly.
"""

from __future__ import annotations

from makima.core.deps import get_app_settings, get_current_user, get_db

__all__ = ["get_app_settings", "get_current_user", "get_db"]