"""Knowledge API routes — manage documents and RAG retrieval."""

from __future__ import annotations

from typing import Any
from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException, UploadFile, File
from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from makima.auth.models import User
from makima.core.deps import get_current_user, get_db
from makima.knowledge.ingest import ingest_document, delete_document_chunks
from makima.knowledge.models import Document
from makima.knowledge.retriever import retrieve, RetrievalResult
from makima_common.logging import get_logger

logger = get_logger(__name__)

router = APIRouter(prefix="/knowledge", tags=["knowledge"])


class DocumentResponse(BaseModel):
    """Document metadata response."""

    id: UUID = Field(..., description="Document ID")
    title: str = Field(..., description="Document title")
    file_type: str = Field(..., description="File type")
    file_size: int = Field(..., description="File size in bytes")
    status: str = Field(..., description="Processing status")
    chunk_count: int = Field(..., description="Number of chunks")
    error: str | None = Field(None, description="Error message if failed")
    created_at: str = Field(..., description="Creation timestamp")


class DocumentListResponse(BaseModel):
    """List of documents."""

    documents: list[DocumentResponse] = Field(default_factory=list)
    count: int = Field(..., description="Total count")


class RetrievalRequest(BaseModel):
    """Retrieval request."""

    query: str = Field(..., min_length=1, description="Search query")
    top_k: int = Field(default=5, ge=1, le=20, description="Max results")


class RetrievalResponse(BaseModel):
    """Single retrieval result."""

    content: str = Field(..., description="Chunk content")
    document_id: str = Field(..., description="Source document ID")
    document_title: str = Field(..., description="Source document title")
    chunk_index: int = Field(..., description="Chunk index in document")
    score: float = Field(..., description="Relevance score")


class RetrievalListResponse(BaseModel):
    """List of retrieval results."""

    results: list[RetrievalResponse] = Field(default_factory=list)
    count: int = Field(..., description="Total count")


@router.post("/documents", response_model=DocumentResponse)
async def upload_document(
    file: UploadFile = File(...),
    user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> DocumentResponse:
    """Upload a document to the knowledge base."""
    if not file.filename:
        raise HTTPException(status_code=400, detail="Filename required")

    # Read file content
    content = await file.read()
    text_content = content.decode("utf-8", errors="replace")

    # Determine file type
    file_type = file.filename.rsplit(".", 1)[-1].lower() if "." in file.filename else "text"

    # Create document record
    doc = Document(
        user_id=user.id,
        title=file.filename,
        file_type=file_type,
        file_size=len(content),
        status="pending",
    )
    db.add(doc)
    await db.flush()

    # Ingest document
    doc = await ingest_document(db, doc, text_content)

    logger.info(
        "Document uploaded",
        document_id=str(doc.id),
        title=doc.title,
        status=doc.status,
    )

    return DocumentResponse(
        id=doc.id,
        title=doc.title,
        file_type=doc.file_type,
        file_size=doc.file_size,
        status=doc.status,
        chunk_count=doc.chunk_count,
        error=doc.error,
        created_at=doc.created_at.isoformat(),
    )


@router.get("/documents", response_model=DocumentListResponse)
async def list_documents(
    user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> DocumentListResponse:
    """List all documents for the current user."""
    stmt = (
        select(Document)
        .where(Document.user_id == user.id)
        .order_by(Document.created_at.desc())
    )
    result = await db.execute(stmt)
    docs = result.scalars().all()

    return DocumentListResponse(
        documents=[
            DocumentResponse(
                id=doc.id,
                title=doc.title,
                file_type=doc.file_type,
                file_size=doc.file_size,
                status=doc.status,
                chunk_count=doc.chunk_count,
                error=doc.error,
                created_at=doc.created_at.isoformat(),
            )
            for doc in docs
        ],
        count=len(docs),
    )


@router.get("/documents/{document_id}", response_model=DocumentResponse)
async def get_document(
    document_id: UUID,
    user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> DocumentResponse:
    """Get a specific document."""
    stmt = select(Document).where(
        Document.id == document_id, Document.user_id == user.id
    )
    result = await db.execute(stmt)
    doc = result.scalar_one_or_none()

    if not doc:
        raise HTTPException(status_code=404, detail="Document not found")

    return DocumentResponse(
        id=doc.id,
        title=doc.title,
        file_type=doc.file_type,
        file_size=doc.file_size,
        status=doc.status,
        chunk_count=doc.chunk_count,
        error=doc.error,
        created_at=doc.created_at.isoformat(),
    )


@router.delete("/documents/{document_id}")
async def delete_document(
    document_id: UUID,
    user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> dict[str, str]:
    """Delete a document and its chunks."""
    stmt = select(Document).where(
        Document.id == document_id, Document.user_id == user.id
    )
    result = await db.execute(stmt)
    doc = result.scalar_one_or_none()

    if not doc:
        raise HTTPException(status_code=404, detail="Document not found")

    # Delete chunks first
    deleted_chunks = await delete_document_chunks(db, document_id)

    # Delete document
    await db.delete(doc)

    logger.info(
        "Document deleted",
        document_id=str(document_id),
        deleted_chunks=deleted_chunks,
    )

    return {"status": "deleted", "id": str(document_id)}


@router.post("/retrieve", response_model=RetrievalListResponse)
async def retrieve_context(
    request: RetrievalRequest,
    user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> RetrievalListResponse:
    """Retrieve relevant document chunks for a query."""
    results = await retrieve(
        db=db,
        query=request.query,
        user_id=user.id,
        top_k=request.top_k,
    )

    return RetrievalListResponse(
        results=[
            RetrievalResponse(
                content=r.content,
                document_id=str(r.document_id),
                document_title=r.document_title,
                chunk_index=r.chunk_index,
                score=r.score,
            )
            for r in results
        ],
        count=len(results),
    )