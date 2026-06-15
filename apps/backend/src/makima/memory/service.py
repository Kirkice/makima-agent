"""Memory service — business logic for long-term memory management."""

from __future__ import annotations

from typing import Any

from makima.clients.memory import MemoryClient, get_memory_client
from makima_common.logging import get_logger

logger = get_logger(__name__)


class MemoryService:
    """High-level memory service for the agent orchestrator."""

    def __init__(self, client: MemoryClient | None = None) -> None:
        self._client = client or get_memory_client()

    @property
    def available(self) -> bool:
        """Check if memory service is available."""
        return self._client.available

    def store_conversation(
        self,
        messages: list[dict[str, str]],
        user_id: str,
        session_id: str = "",
    ) -> list[dict[str, Any]]:
        """Store a conversation into long-term memory.

        Extracts key facts, preferences, and context from the conversation
        and stores them as memories scoped to the user.

        Args:
            messages: Conversation messages with 'role' and 'content'.
            user_id: User identifier.
            session_id: Optional session identifier for metadata.

        Returns:
            List of created memory entries.
        """
        if not self._client.available:
            logger.debug("Memory service unavailable, skipping store")
            return []

        metadata = {"session_id": session_id, "source": "conversation"}
        memories = self._client.add(messages, user_id=user_id, metadata=metadata)
        logger.info(
            "Stored conversation memories",
            user_id=user_id,
            count=len(memories),
        )
        return memories

    def recall(self, query: str, user_id: str, limit: int = 5) -> list[dict[str, Any]]:
        """Recall relevant memories for a query.

        Args:
            query: The user's current input or question.
            user_id: User identifier.
            limit: Maximum number of memories to retrieve.

        Returns:
            List of relevant memory entries with 'memory' and 'score' fields.
        """
        if not self._client.available:
            logger.debug("Memory service unavailable, skipping recall")
            return []

        memories = self._client.search(query, user_id=user_id, limit=limit)
        logger.info(
            "Recalled memories",
            user_id=user_id,
            query=query[:50],
            count=len(memories),
        )
        return memories

    def get_user_memories(self, user_id: str) -> list[dict[str, Any]]:
        """Get all memories for a user.

        Args:
            user_id: User identifier.

        Returns:
            List of all memory entries.
        """
        if not self._client.available:
            return []
        return self._client.get_all(user_id=user_id)

    def delete_memory(self, memory_id: str) -> bool:
        """Delete a specific memory entry.

        Args:
            memory_id: Memory entry ID.

        Returns:
            True if deleted successfully.
        """
        if not self._client.available:
            return False
        return self._client.delete(memory_id)

    def format_memories_for_prompt(self, memories: list[dict[str, Any]]) -> str:
        """Format memories into a string suitable for injection into a system prompt.

        Args:
            memories: List of memory entries from recall().

        Returns:
            Formatted string, or empty string if no memories.
        """
        if not memories:
            return ""

        lines = ["<user_memories>"]
        for mem in memories:
            text = mem.get("memory", "")
            score = mem.get("score", 0)
            if text:
                lines.append(f"- {text} (relevance: {score:.2f})")
        lines.append("</user_memories>")
        return "\n".join(lines)