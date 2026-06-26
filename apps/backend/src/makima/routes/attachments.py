"""Attachment upload routes.

Provides a multipart upload endpoint that saves files to a controlled directory
inside tool_working_dir so they can later be read by the agent's file tools
without violating path-traversal security.
"""

from __future__ import annotations

import mimetypes
import re
import uuid
from pathlib import Path

from fastapi import APIRouter, Depends, HTTPException, UploadFile, File, Form, status
from pydantic import BaseModel
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from makima.auth.models import User
from makima.core.deps import get_current_user, get_db
from makima.sessions.models import Session
from makima_common.config import get_settings

import shutil
import time

router = APIRouter(prefix="/attachments", tags=["attachments"])

# Maximum upload size: 10 MB
MAX_UPLOAD_SIZE = 10 * 1024 * 1024

# Cleanup: remove attachments older than 7 days
ATTACHMENT_MAX_AGE_SECONDS = 7 * 24 * 60 * 60

# Extensions we treat as text (safe to inline into context)
TEXT_EXTENSIONS: set[str] = {
    ".txt", ".md", ".json", ".yaml", ".yml",
    ".py", ".rs", ".js", ".ts", ".tsx", ".jsx",
    ".html", ".css", ".cs", ".java", ".cpp", ".h",
    ".toml", ".xml", ".csv", ".sh", ".bash", ".zsh",
    ".sql", ".graphql", ".proto", ".ini", ".cfg",
    ".go", ".rb", ".php", ".swift", ".kt", ".scala",
    ".lua", ".r", ".m", ".mm", ".vue", ".svelte",
    ".env", ".gitignore", ".dockerignore", ".editorconfig",
    ".lock", ".log",
}


class AttachmentUploadResponse(BaseModel):
    attachment_id: str
    original_name: str
    stored_path: str  # relative path under tool_working_dir
    mime_type: str
    size: int
    is_text: bool


def _sanitize_filename(name: str) -> str:
    """Remove path separators and limit length."""
    # Strip any directory components
    name = Path(name).name
    # Replace unsafe characters
    name = re.sub(r"[^\w\-. ]", "_", name)
    # Limit length
    if len(name) > 200:
        stem = name[:180]
        suffix = Path(name).suffix
        name = stem + suffix
    return name or "unnamed"


def _is_text_file(filename: str, mime_type: str) -> bool:
    """Determine if a file should be treated as text."""
    ext = Path(filename).suffix.lower()
    if ext in TEXT_EXTENSIONS:
        return True
    # Fallback: check MIME type
    if mime_type.startswith("text/"):
        return True
    if mime_type in (
        "application/json",
        "application/xml",
        "application/yaml",
        "application/x-yaml",
        "application/toml",
        "application/javascript",
        "application/typescript",
        "application/x-sh",
    ):
        return True
    return False


def _attachments_dir(session_id: str) -> Path:
    """Return the controlled directory for a session's attachments."""
    settings = get_settings()
    base = Path(settings.tool_working_dir).resolve()
    return base / "attachments" / session_id


@router.post("/upload", response_model=AttachmentUploadResponse, status_code=status.HTTP_201_CREATED)
async def upload_attachment(
    file: UploadFile = File(...),
    session_id: str = Form(...),
    user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
) -> AttachmentUploadResponse:
    """Upload a file attachment for a chat session.
    
    Files are saved under {tool_working_dir}/attachments/{session_id}/
    so they remain within the agent's sandboxed working directory.
    """
    settings = get_settings()
    
    # Validate session_id format
    try:
        session_uuid = uuid.UUID(session_id)
    except ValueError:
        raise HTTPException(status_code=400, detail="Invalid session_id format")
    
    # Verify session exists and belongs to current user
    result = await db.execute(
        select(Session).where(
            Session.id == session_uuid,
            Session.user_id == user.id,
        )
    )
    session = result.scalar_one_or_none()
    if not session:
        raise HTTPException(
            status_code=404,
            detail="Session not found or access denied",
        )

    # Read file content with size limit
    content = await file.read()
    size = len(content)

    if size == 0:
        raise HTTPException(status_code=400, detail="Empty file")
    if size > MAX_UPLOAD_SIZE:
        raise HTTPException(
            status_code=413,
            detail=f"File too large. Maximum size is {MAX_UPLOAD_SIZE // (1024*1024)} MB",
        )

    # Determine MIME type
    filename = file.filename or "unnamed"
    mime_type = file.content_type or mimetypes.guess_type(filename)[0] or "application/octet-stream"

    # Sanitize filename
    safe_name = _sanitize_filename(filename)

    # Generate unique attachment ID and stored filename
    attachment_id = str(uuid.uuid4())
    stored_filename = f"{attachment_id[:8]}_{safe_name}"

    # Save to controlled directory
    dest_dir = _attachments_dir(session_id)
    dest_dir.mkdir(parents=True, exist_ok=True)
    dest_path = dest_dir / stored_filename

    # Write file
    dest_path.write_bytes(content)

    # Compute relative path from tool_working_dir
    base = Path(settings.tool_working_dir).resolve()
    stored_rel = str(dest_path.resolve().relative_to(base))

    is_text = _is_text_file(filename, mime_type)

    return AttachmentUploadResponse(
        attachment_id=attachment_id,
        original_name=filename,
        stored_path=stored_rel,
        mime_type=mime_type,
        size=size,
        is_text=is_text,
    )


# ── Cleanup ────────────────────────────────────────────────────────

def cleanup_old_attachments() -> int:
    """Remove attachment session directories older than ATTACHMENT_MAX_AGE_SECONDS.

    Called during app startup. Returns the number of session dirs removed.
    """
    from makima_common.logging import get_logger

    settings = get_settings()
    attachments_root = Path(settings.tool_working_dir).resolve() / "attachments"
    if not attachments_root.exists():
        return 0

    now = time.time()
    removed = 0
    cutoff = now - ATTACHMENT_MAX_AGE_SECONDS

    for session_dir in attachments_root.iterdir():
        if not session_dir.is_dir():
            continue
        try:
            dir_mtime = session_dir.stat().st_mtime
            if dir_mtime < cutoff:
                shutil.rmtree(session_dir, ignore_errors=True)
                removed += 1
        except OSError:
            pass

    if removed:
        logger = get_logger(__name__)
        logger.info(f"Cleaned up {removed} old attachment session(s)")

    return removed
