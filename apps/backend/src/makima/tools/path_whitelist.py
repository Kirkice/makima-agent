"""Session-level path whitelist for file access control.

This module provides a session-scoped whitelist that remembers user-approved
paths within the same session, avoiding repeated approval requests for the
same path.
"""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from pathlib import Path
from threading import Lock

from makima_common.logging import get_logger

logger = get_logger(__name__)


@dataclass
class WhitelistEntry:
    """A single whitelist entry."""

    path: str
    session_id: str
    approved_at: float = field(default_factory=time.time)
    operations: set[str] = field(default_factory=set)  # "read", "write", "list"


class PathWhitelist:
    """Session-level path whitelist.

    Features:
    - Remembers user-approved paths within the same session
    - Supports operation-level granularity (read/write/list)
    - Thread-safe for concurrent access
    - Optional TTL (time-to-live) for entries
    """

    def __init__(self, ttl_seconds: float | None = None) -> None:
        """Initialize the whitelist.

        Args:
            ttl_seconds: Optional time-to-live for whitelist entries.
                        If None, entries persist for the entire session.
        """
        self._lock = Lock()
        self._entries: dict[str, list[WhitelistEntry]] = {}  # session_id -> entries
        self._ttl = ttl_seconds

    def is_allowed(
        self,
        session_id: str,
        path: str | Path,
        operation: str = "read",
    ) -> bool:
        """Check if a path is allowed for the given session.

        Args:
            session_id: The session ID to check for
            path: The path to check (can be string or Path)
            operation: The operation type ("read", "write", "list")

        Returns:
            True if the path is allowed, False otherwise
        """
        path_str = str(Path(path).resolve())

        with self._lock:
            entries = self._entries.get(session_id, [])

            # Clean up expired entries
            if self._ttl is not None:
                now = time.time()
                entries = [e for e in entries if now - e.approved_at < self._ttl]
                self._entries[session_id] = entries

            # Check if any entry matches
            for entry in entries:
                entry_path = Path(entry.path).resolve()
                target_path = Path(path_str).resolve()

                # Check if target is within the whitelisted directory
                try:
                    target_path.relative_to(entry_path)
                    # Path is within the whitelisted directory
                    if operation in entry.operations or "*" in entry.operations:
                        logger.debug(
                            "Path allowed via session whitelist",
                            session_id=session_id,
                            path=path_str,
                            operation=operation,
                            whitelisted_path=str(entry_path),
                        )
                        return True
                except ValueError:
                    # Path is not within the whitelisted directory
                    continue

            return False

    def add(
        self,
        session_id: str,
        path: str | Path,
        operations: set[str] | None = None,
    ) -> None:
        """Add a path to the whitelist for a session.

        Args:
            session_id: The session ID
            path: The path to whitelist
            operations: Set of allowed operations (default: {"read", "write", "list"})
        """
        path_str = str(Path(path).resolve())
        ops = operations or {"read", "write", "list"}

        with self._lock:
            if session_id not in self._entries:
                self._entries[session_id] = []

            # Check if entry already exists
            for entry in self._entries[session_id]:
                if entry.path == path_str:
                    # Merge operations
                    entry.operations.update(ops)
                    logger.debug(
                        "Updated whitelist entry",
                        session_id=session_id,
                        path=path_str,
                        operations=entry.operations,
                    )
                    return

            # Add new entry
            entry = WhitelistEntry(
                path=path_str,
                session_id=session_id,
                operations=ops,
            )
            self._entries[session_id].append(entry)

            logger.info(
                "Added path to session whitelist",
                session_id=session_id,
                path=path_str,
                operations=ops,
            )

    def remove(self, session_id: str, path: str | Path | None = None) -> int:
        """Remove entries from the whitelist.

        Args:
            session_id: The session ID
            path: Optional specific path to remove. If None, removes all entries
                  for the session.

        Returns:
            Number of entries removed
        """
        with self._lock:
            if session_id not in self._entries:
                return 0

            if path is None:
                # Remove all entries for session
                count = len(self._entries[session_id])
                del self._entries[session_id]
                logger.info(
                    "Cleared all whitelist entries for session",
                    session_id=session_id,
                    count=count,
                )
                return count

            # Remove specific path
            path_str = str(Path(path).resolve())
            original_count = len(self._entries[session_id])
            self._entries[session_id] = [
                e for e in self._entries[session_id] if e.path != path_str
            ]
            removed = original_count - len(self._entries[session_id])

            if removed > 0:
                logger.info(
                    "Removed path from session whitelist",
                    session_id=session_id,
                    path=path_str,
                )

            return removed

    def get_entries(self, session_id: str) -> list[WhitelistEntry]:
        """Get all whitelist entries for a session.

        Args:
            session_id: The session ID

        Returns:
            List of whitelist entries
        """
        with self._lock:
            return list(self._entries.get(session_id, []))

    def clear_all(self) -> int:
        """Clear all whitelist entries.

        Returns:
            Total number of entries cleared
        """
        with self._lock:
            count = sum(len(entries) for entries in self._entries.values())
            self._entries.clear()
            logger.info("Cleared all whitelist entries", count=count)
            return count

    # Convenience aliases for simpler API
    def is_path_allowed(self, session_id: str, path: str | Path) -> bool:
        """Check if a path is allowed (alias for is_allowed with any operation)."""
        return self.is_allowed(session_id, path, "*")

    def add_path(self, session_id: str, path: str | Path) -> None:
        """Add a path to the whitelist (alias for add with all operations)."""
        self.add(session_id, path, {"*"})


# Global singleton instance
_whitelist_instance: PathWhitelist | None = None


def get_path_whitelist() -> PathWhitelist:
    """Get the global PathWhitelist singleton.

    Returns:
        The global PathWhitelist instance
    """
    global _whitelist_instance
    if _whitelist_instance is None:
        _whitelist_instance = PathWhitelist()
    return _whitelist_instance


def reset_path_whitelist() -> None:
    """Reset the global PathWhitelist instance (for testing)."""
    global _whitelist_instance
    if _whitelist_instance is not None:
        _whitelist_instance.clear_all()
    _whitelist_instance = None