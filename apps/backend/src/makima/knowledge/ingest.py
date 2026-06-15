"""Document ingestion pipeline — parse, chunk, embed, store."""

from __future__ import annotations

import uuid
from typing import Any

from sqlalchemy.ext.asyncio import AsyncSession

from makima.clients.llm import get_chat_model
from makima.knowledge.models import Document, DocumentChunk
from makima_common.config import get_settings
from makima_common.logging import get_logger

logger = get_logger(__name__)


def _chunk_text(text: str, chunk_size: int, overlap: int) -> list[str]:
    """Split text into overlapping chunks.

    Args:
        text: Full text content.
        chunk_size: Target characters per chunk.
        overlap: Number of overlapping characters between chunks.

    Returns:
        List of text chunks.
    """
    if not text:
        return []

    chunks: list[str] = []
    start = 0
    while start < len(text):
        end = start + chunk_size
        chunk = text[start:end]
        if chunk.strip():
            chunks.append(chunk.strip())
        start = end - overlap
    return chunks


def _estimate_tokens(text: str) -> int:
    """Rough token count estimate (1 token ≈ 4 chars)."""
    return len(text) // 4


async def ingest_document(
    db: AsyncSession,
    document: Document,
    content: str,
) -> Document:
    """Process and ingest a document into the knowledge base.

    Steps:
    1. Update document status to 'processing'
    2. Split content into chunks
    3. Store chunks in database
    4. Generate embeddings and store (when pgvector is available)
    5. Update document status to 'ready'

    Args:
        db: Database session.
        document: Document ORM object.
        content: Raw text content of the document.

    Returns:
        Updated Document object.
    """
    settings = get_settings()

    try:
        # Mark as processing
        document.status = "processing"
        await db.flush()

        # Chunk the document
        chunks = _chunk_text(
            content,
            chunk_size=settings.rag_chunk_size,
            overlap=settings.rag_chunk_overlap,
        )

        if not chunks:
            document.status = "failed"
            document.error = "No content to process"
            await db.flush()
            return document

        # Store chunks
        for i, chunk_text in enumerate(chunks):
            chunk = DocumentChunk(
                document_id=document.id,
                chunk_index=i,
                content=chunk_text,
                token_count=_estimate_tokens(chunk_text),
                metadata_={"source": document.title, "chunk_index": i},
            )
            db.add(chunk)

        # Update document metadata
        document.chunk_count = len(chunks)
        document.file_size = len(content)
        document.status = "ready"
        document.error = None
        await db.flush()

        logger.info(
            "Document ingested",
            document_id=str(document.id),
            title=document.title,
            chunks=len(chunks),
        )

    except Exception as e:
        document.status = "failed"
        document.error = str(e)
        await db.flush()
        logger.error(
            "Document ingestion failed",
            document_id=str(document.id),
            error=str(e),
        )

    return document


async def delete_document_chunks(db: AsyncSession, document_id: uuid.UUID) -> int:
    """Delete all chunks for a document.

    Args:
        db: Database session.
        document_id: Document UUID.

    Returns:
        Number of chunks deleted.
    """
    from sqlalchemy import delete

    stmt = delete(DocumentChunk).where(DocumentChunk.document_id == document_id)
    result = await db.execute(stmt)
    return result.rowcount  # type: ignore[return-value]