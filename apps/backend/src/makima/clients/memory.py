"""Mem0 memory client wrapper."""

from __future__ import annotations

from functools import lru_cache
from typing import Any

from makima_common.config import get_settings
from makima_common.logging import get_logger

logger = get_logger(__name__)


class MemoryClient:
    """Wrapper around Mem0 for long-term memory management."""

    def __init__(self) -> None:
        self._memory: Any = None
        self._initialized = False

    def _ensure_initialized(self) -> None:
        """Lazy-initialize Mem0 to avoid import issues when mem0ai is not installed."""
        if self._initialized:
            return
        try:
            from mem0 import Memory

            settings = get_settings()
            config: dict[str, Any] = {
                "embedder": {
                    "provider": "openai",
                    "config": {
                        "model": settings.mem0_embedding_model,
                        "api_key": settings.llm_api_key,
                    },
                },
                "vector_store": {
                    "provider": "pgvector",
                    "config": {
                        "dbname": settings.postgres_db,
                        "user": settings.postgres_user,
                        "password": settings.postgres_password,
                        "host": "localhost",
                        "port": settings.postgres_port,
                        "collection_name": "makima_memories",
                    },
                },
            }
            self._memory = Memory.from_config(config)
            self._initialized = True
            logger.info("Mem0 memory client initialized")
        except ImportError:
            logger.warning("mem0ai not installed, memory features disabled")
            self._initialized = True
        except Exception as e:
            logger.error("Failed to initialize Mem0", error=str(e))
            self._initialized = True

    @property
    def available(self) -> bool:
        """Check if memory client is available."""
        self._ensure_initialized()
        return self._memory is not None

    def add(
        self,
        messages: list[dict[str, str]],
        user_id: str,
        metadata: dict[str, Any] | None = None,
    ) -> list[dict[str, Any]]:
        """Add memories from conversation messages.

        Args:
            messages: List of message dicts with 'role' and 'content'.
            user_id: User identifier for scoping memories.
            metadata: Optional metadata to attach.

        Returns:
            List of created memory entries.
        """
        self._ensure_initialized()
        if not self._memory:
            return []
        try:
            result = self._memory.add(messages, user_id=user_id, metadata=metadata or {})
            return result.get("results", []) if isinstance(result, dict) else []
        except Exception as e:
            logger.error("Failed to add memory", error=str(e), user_id=user_id)
            return []

    def search(self, query: str, user_id: str, limit: int = 5) -> list[dict[str, Any]]:
        """Search memories relevant to a query.

        Args:
            query: Search query text.
            user_id: User identifier for scoping.
            limit: Maximum number of results.

        Returns:
            List of matching memory entries.
        """
        self._ensure_initialized()
        if not self._memory:
            return []
        try:
            result = self._memory.search(query, user_id=user_id, limit=limit)
            return result.get("results", []) if isinstance(result, dict) else []
        except Exception as e:
            logger.error("Failed to search memory", error=str(e), user_id=user_id)
            return []

    def get_all(self, user_id: str) -> list[dict[str, Any]]:
        """Get all memories for a user.

        Args:
            user_id: User identifier.

        Returns:
            List of all memory entries.
        """
        self._ensure_initialized()
        if not self._memory:
            return []
        try:
            result = self._memory.get_all(user_id=user_id)
            return result.get("results", []) if isinstance(result, dict) else []
        except Exception as e:
            logger.error("Failed to get memories", error=str(e), user_id=user_id)
            return []

    def delete(self, memory_id: str) -> bool:
        """Delete a specific memory.

        Args:
            memory_id: Memory entry ID.

        Returns:
            True if deleted successfully.
        """
        self._ensure_initialized()
        if not self._memory:
            return False
        try:
            self._memory.delete(memory_id)
            return True
        except Exception as e:
            logger.error("Failed to delete memory", error=str(e), memory_id=memory_id)
            return False


@lru_cache
def get_memory_client() -> MemoryClient:
    """Return a cached MemoryClient instance."""
    return MemoryClient()