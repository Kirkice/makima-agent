"""Attachment context builder — reads uploaded text files and formats them for the LLM."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from makima_common.config import get_settings
from makima_common.logging import get_logger

if TYPE_CHECKING:
    from makima_schemas.api import AttachmentInfo

logger = get_logger(__name__)

# Limits
MAX_SINGLE_FILE_BYTES = 50 * 1024  # 50 KB per file
MAX_TOTAL_ATTACHMENT_BYTES = 200 * 1024  # 200 KB total


def build_attachment_context(attachments: list[AttachmentInfo]) -> str:
    """Read text attachments and build a context string to prepend to user input.

    For text files: reads content (up to MAX_SINGLE_FILE_BYTES), truncates if needed.
    For binary files: only includes metadata (name, type, size).

    Returns an empty string if no attachments.
    """
    if not attachments:
        return ""

    settings = get_settings()
    base_dir = Path(settings.tool_working_dir).resolve()

    lines: list[str] = []
    total_bytes = 0
    file_count = 0

    for att in attachments:
        file_count += 1

        if att.is_text:
            # Try to read the file content
            # SECURITY: resolve the path and verify it stays within base_dir
            # Never trust client-supplied stored_path
            raw_path = base_dir / att.stored_path
            try:
                file_path = raw_path.resolve()
                if not str(file_path).startswith(str(base_dir)):
                    logger.warning(
                        "Path traversal blocked for attachment",
                        stored_path=att.stored_path,
                        resolved=str(file_path),
                    )
                    lines.append(
                        f"[{file_count}] {att.original_name} ({_format_size(att.size)}, {att.mime_type})\n"
                        f"Note: Invalid path — rejected by server.\n"
                    )
                    continue

                if not file_path.exists():
                    lines.append(
                        f"[{file_count}] {att.original_name} ({_format_size(att.size)}, {att.mime_type})\n"
                        f"Path: {att.stored_path}\n"
                        f"Note: File not found on server.\n"
                    )
                    continue

                content = file_path.read_text(encoding="utf-8", errors="replace")
                content_bytes = len(content.encode("utf-8"))

                # Check total limit
                if total_bytes + content_bytes > MAX_TOTAL_ATTACHMENT_BYTES:
                    lines.append(
                        f"[{file_count}] {att.original_name} ({_format_size(att.size)}, {att.mime_type})\n"
                        f"Path: {att.stored_path}\n"
                        f"Note: Skipped — total attachment context limit reached ({MAX_TOTAL_ATTACHMENT_BYTES // 1024} KB).\n"
                    )
                    continue

                # Truncate if single file is too large
                truncated = False
                if content_bytes > MAX_SINGLE_FILE_BYTES:
                    # Truncate by bytes
                    content = content.encode("utf-8")[:MAX_SINGLE_FILE_BYTES].decode("utf-8", errors="ignore")
                    truncated = True

                total_bytes += len(content.encode("utf-8"))

                trunc_note = f"\n[truncated to {MAX_SINGLE_FILE_BYTES // 1024} KB]" if truncated else ""
                lines.append(
                    f"[{file_count}] {att.original_name} ({_format_size(att.size)}, {att.mime_type})\n"
                    f"Path: {att.stored_path}\n"
                    f"Content:{trunc_note}\n```\n{content}\n```\n"
                )

            except Exception as e:
                logger.warning("Failed to read attachment", path=att.stored_path, error=str(e))
                lines.append(
                    f"[{file_count}] {att.original_name} ({_format_size(att.size)}, {att.mime_type})\n"
                    f"Path: {att.stored_path}\n"
                    f"Note: Failed to read file content.\n"
                )
        else:
            # Binary file — only metadata
            lines.append(
                f"[{file_count}] {att.original_name} ({_format_size(att.size)}, {att.mime_type})\n"
                f"Path: {att.stored_path}\n"
                f"Note: Binary attachment uploaded but not inlined into context.\n"
            )

    if not lines:
        return ""

    header = "--- Attached Files ---\n"
    footer = "--- End Attachments ---\n"
    return header + "\n".join(lines) + footer


def _format_size(size_bytes: int) -> str:
    """Format byte size to human-readable string."""
    if size_bytes < 1024:
        return f"{size_bytes} B"
    elif size_bytes < 1024 * 1024:
        return f"{size_bytes / 1024:.1f} KB"
    else:
        return f"{size_bytes / (1024 * 1024):.1f} MB"