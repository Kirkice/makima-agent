"""Backward-compatible re-exports from domain models.

DEPRECATED: Import from domain-specific modules directly:
- User: ``makima.auth.models``
- Session, Message: ``makima.sessions.models``
- Task: ``makima.tasks.models``
"""

from __future__ import annotations

from makima.auth.models import User
from makima.sessions.models import Message, Session
from makima.tasks.models import Task

__all__ = ["User", "Session", "Message", "Task"]