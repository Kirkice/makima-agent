"""File system tools."""

from __future__ import annotations

import os
from pathlib import Path

from langchain_core.tools import tool

from makima_common.config import get_settings
from makima_common.logging import get_logger

logger = get_logger(__name__)


def _safe_path(file_path: str) -> Path:
    """Validate and return a safe path within the working directory."""
    settings = get_settings()
    base = Path(settings.tool_working_dir).resolve()
    target = (base / file_path).resolve()

    # Prevent path traversal
    if not str(target).startswith(str(base)):
        raise ValueError(f"Access denied: path traversal detected for {file_path}")

    return target


@tool
def read_file(file_path: str) -> str:
    """Read the contents of a file.

    Args:
        file_path: Relative path to the file within the working directory.

    Returns:
        The file contents as a string.
    """
    target = _safe_path(file_path)
    if not target.exists():
        return f"Error: File not found: {file_path}"
    if not target.is_file():
        return f"Error: Not a file: {file_path}"
    try:
        return target.read_text(encoding="utf-8")
    except Exception as e:
        return f"Error reading file: {e}"


@tool
def write_file(file_path: str, content: str) -> str:
    """Write content to a file. Creates the file if it doesn't exist.

    Args:
        file_path: Relative path to the file within the working directory.
        content: The content to write.

    Returns:
        Success or error message.
    """
    target = _safe_path(file_path)
    try:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")
        return f"Successfully wrote {len(content)} characters to {file_path}"
    except Exception as e:
        return f"Error writing file: {e}"


@tool
def list_directory(dir_path: str = ".") -> str:
    """List files and directories.

    Args:
        dir_path: Relative path to the directory within the working directory.

    Returns:
        A formatted list of directory contents.
    """
    target = _safe_path(dir_path)
    if not target.exists():
        return f"Error: Directory not found: {dir_path}"
    if not target.is_dir():
        return f"Error: Not a directory: {dir_path}"
    try:
        entries = sorted(target.iterdir())
        if not entries:
            return f"{dir_path} is empty"
        lines = []
        for entry in entries:
            prefix = "[DIR] " if entry.is_dir() else "      "
            lines.append(f"{prefix}{entry.name}")
        return "\n".join(lines)
    except Exception as e:
        return f"Error listing directory: {e}"