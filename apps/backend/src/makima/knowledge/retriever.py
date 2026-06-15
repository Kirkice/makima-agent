"""RAG retriever — search document chunks for relevant context."""

from __future__ import annotations

import uuid
from typing import Any

from sqlalchemy import select, text
from sqlalchemy.ext.asyncio import AsyncSession

from makima.clients.llm import get_chat_model
from makima.knowledge.models import Document, DocumentChunk
from makima_common.config import get_settings
from makima_common.logging import get_logger

logger = get_logger(__name__)


class RetrievalResult:
    """A single retrieval result."""

    def __init__(
        self,
        content: str,
        document_id: uuid.UUID,
        document_title: str,
        chunk_index: int,
        score: float = 0.0,
        metadata: dict[str, Any] | None = None,
    ) -> None:
        self.content = content
        self.document_id = document_id
        self.document_title = document_title
        self.chunk_index = chunk_index
        self.score = score
        self.metadata = metadata or {}

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "content": self.content,
            "document_id": str(self.document_id),
            "document_title": self.document_title,
            "chunk_index": self.chunk_index,
            "score": self.score,
            "metadata": self.metadata,
        }


async def retrieve(
    db: AsyncSession,
    query: str,
    user_id: uuid.UUID,
    top_k: int = 5,
    min_score: float = 0.0,
) -> list[RetrievalResult]:
    """Retrieve relevant document chunks for a query.

    Currently uses simple keyword matching. Can be extended to use
    vector similarity search when embeddings are stored.

    Args:
        db: Database session.
        query: Search query.
        user_id: User ID to scope search.
        top_k: Maximum number of results.
        min_score: Minimum relevance score.

    Returns:
        List of RetrievalResult objects.
    """
    settings = get_settings()

    # Get user's documents
    stmt = (
        select(Document.id)
        .where(Document.user_id == user_id)
        .where(Document.status == "ready")
    )
    result = await db.execute(stmt)
    document_ids = [row[0] for row in result.fetchall()]

    if not document_ids:
        logger.debug("No documents available for retrieval", user_id=str(user_id))
        return []

    # Search chunks using ILIKE (case-insensitive keyword matching)
    # Split query into keywords for better matching
    keywords = query.lower().split()

    stmt = (
        select(DocumentChunk, Document.title)
        .join(Document, DocumentChunk.document_id == Document.id)
        .where(DocumentChunk.document_id.in_(document_ids))
    )

    # Add keyword filters
    for keyword in keywords:
        if len(keyword) > 2:  # Skip very short words
            stmt = stmt.where(DocumentChunk.content.ilike(f"%{keyword}%"))

    stmt = stmt.limit(top_k * 2)  # Get more to score
    result = await db.execute(stmt)
    rows = result.fetchall()

    if not rows:
        logger.debug("No chunks found", query=query[:50], user_id=str(user_id))
        return []

    # Score results based on keyword frequency
    results: list[RetrievalResult] = []
    for chunk, doc_title in rows:
        content_lower = chunk.content.lower()
        score = sum(1 for kw in keywords if kw in content_lower) / max(len(keywords), 1)

        if score >= min_score:
            results.append(
                RetrievalResult(
                    content=chunk.content,
                    document_id=chunk.document_id,
                    document_title=doc_title,
                    chunk_index=chunk.chunk_index,
                    score=score,
                    metadata=chunk.metadata_,
                )
            )

    # Sort by score descending
    results.sort(key=lambda r: r.score, reverse=True)
    results = results[:top_k]

    logger.info(
        "Retrieved chunks",
        query=query[:50],
        user_id=str(user_id),
        count=len(results),
    )

    return results


def format_context_for_prompt(results: list[RetrievalResult]) -> str:
    """Format retrieval results into a context string for LLM prompts.

    Args:
        results: List of RetrievalResult objects.

    Returns:
        Formatted context string, or empty string if no results.
    """
    if not results:
        return ""

    lines = ["<retrieved_context>"]
    for i, result in enumerate(results, 1):
        lines.append(f"[{i}] Source: {result.document_title} (chunk {result.chunk_index})")
        lines.append(result.content)
        lines.append("")
    lines.append("</retrieved_context>")
    return "\n".join(lines)