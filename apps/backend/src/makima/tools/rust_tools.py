"""Rust-accelerated tool wrappers.

These wrappers try to use the Rust gRPC tool runtime first,
and fall back to Python implementations if Rust is unavailable.
"""

from __future__ import annotations

import asyncio
from pathlib import Path
from typing import Any

from langchain_core.tools import tool

from makima.clients.rust_client import get_rust_client
from makima.tools.file_tool import read_file as py_read_file
from makima.tools.file_tool import write_file as py_write_file
from makima.tools.file_tool import list_directory as py_list_directory
from makima.tools.shell_tool import execute_shell as py_execute_shell
from makima.tools.http_tool import http_request as py_http_request
from makima_common.config import get_settings
from makima_common.logging import get_logger

logger = get_logger(__name__)


def _path_is_inside(path: Path, base: Path) -> bool:
    try:
        path.relative_to(base)
        return True
    except ValueError:
        return False


def _requires_python_path_handling(path_str: str) -> bool:
    """Return True when the Python file tools should handle this path.

    Rust runtime currently only supports a single base_dir sandbox. If the path
    is outside tool_working_dir but inside a configured allowed directory, we
    must bypass Rust and use the Python implementation that understands the
    whitelist rules.
    """
    settings = get_settings()
    sandbox_base = Path(settings.tool_working_dir).resolve()
    raw_path = Path(path_str)
    target = (sandbox_base / raw_path).resolve() if not raw_path.is_absolute() else raw_path.resolve()

    if _path_is_inside(target, sandbox_base):
        return False

    for allowed_dir in settings.tool_allowed_dirs:
        allowed_base = Path(allowed_dir).resolve()
        if _path_is_inside(target, allowed_base):
            return True

    return False


@tool
async def read_file(file_path: str) -> str:
    """Read the contents of a file at the given path.

    Args:
        file_path: The path to the file to read (relative or absolute).

    Returns:
        The contents of the file as a string, or an error message.
    """
    if _requires_python_path_handling(file_path):
        return await py_read_file.ainvoke({"file_path": file_path})

    rust = get_rust_client()
    if await rust.is_available():
        try:
            settings = get_settings()
            result = await rust.read_file(file_path, settings.tool_working_dir)
            if result["success"]:
                return result["content"]
            logger.warning("Rust read_file failed, falling back", error=result.get("error"))
        except Exception as e:
            logger.warning("Rust read_file exception, falling back", error=str(e))
    # Fallback to Python
    return await py_read_file.ainvoke({"file_path": file_path})


@tool
async def write_file(file_path: str, content: str) -> str:
    """Write content to a file at the given path.

    Args:
        file_path: The path to the file to write (relative or absolute).
        content: The content to write to the file.

    Returns:
        A success message or an error message.
    """
    if _requires_python_path_handling(file_path):
        return await py_write_file.ainvoke({"file_path": file_path, "content": content})

    rust = get_rust_client()
    if await rust.is_available():
        try:
            settings = get_settings()
            result = await rust.write_file(file_path, content, settings.tool_working_dir)
            if result["success"]:
                return f"Successfully wrote {result['bytes_written']} bytes to {file_path}"
            logger.warning("Rust write_file failed, falling back", error=result.get("error"))
        except Exception as e:
            logger.warning("Rust write_file exception, falling back", error=str(e))
    # Fallback to Python
    return await py_write_file.ainvoke({"file_path": file_path, "content": content})


@tool
async def list_directory(path: str = ".") -> str:
    """List the contents of a directory.

    Args:
        path: The directory path to list (relative or absolute). Defaults to current directory.

    Returns:
        A formatted listing of the directory contents, or an error message.
    """
    if _requires_python_path_handling(path):
        return await py_list_directory.ainvoke({"dir_path": path})

    rust = get_rust_client()
    if await rust.is_available():
        try:
            settings = get_settings()
            result = await rust.list_directory(path, settings.tool_working_dir)
            if result["success"]:
                lines = []
                for entry in result["entries"]:
                    prefix = "📁 " if entry["is_dir"] else "📄 "
                    size_str = f" ({entry['size']} bytes)" if not entry["is_dir"] else ""
                    lines.append(f"{prefix}{entry['name']}{size_str}")
                return "\n".join(lines) if lines else "(empty directory)"
            logger.warning("Rust list_directory failed, falling back", error=result.get("error"))
        except Exception as e:
            logger.warning("Rust list_directory exception, falling back", error=str(e))
    # Fallback to Python
    return await py_list_directory.ainvoke({"path": path})


@tool
async def execute_shell(command: str) -> str:
    """Execute a shell command and return the output.

    Args:
        command: The shell command to execute.

    Returns:
        The combined stdout and stderr output from the command.
    """
    rust = get_rust_client()
    if await rust.is_available():
        try:
            settings = get_settings()
            result = await rust.execute_shell(
                command,
                settings.tool_working_dir,
                timeout_seconds=settings.tool_timeout,
            )
            if result.get("blocked"):
                return f"⛔ Command blocked: {result['block_reason']}"
            output = ""
            if result["stdout"]:
                output += result["stdout"]
            if result["stderr"]:
                output += f"\nSTDERR: {result['stderr']}"
            if result["exit_code"] != 0:
                output += f"\n(exit code: {result['exit_code']})"
            return output.strip() or "(no output)"
        except Exception as e:
            logger.warning("Rust execute_shell exception, falling back", error=str(e))
    # Fallback to Python
    return await py_execute_shell.ainvoke({"command": command})


@tool
async def http_request(url: str, method: str = "GET", body: str = "") -> str:
    """Send an HTTP request and return the response.

    Args:
        url: The URL to send the request to.
        method: HTTP method (GET, POST, PUT, DELETE). Defaults to GET.
        body: Request body for POST/PUT requests. Defaults to empty.

    Returns:
        The response body, or an error message.
    """
    rust = get_rust_client()
    if await rust.is_available():
        try:
            result = await rust.http_request(
                url=url,
                method=method,
                body=body,
                timeout_seconds=30,
            )
            if result.get("blocked"):
                return f"⛔ Request blocked: {result['block_reason']}"
            if result["success"]:
                return f"Status: {result['status_code']}\n{result['body']}"
            return f"HTTP {result['status_code']}: {result['body']}"
        except Exception as e:
            logger.warning("Rust http_request exception, falling back", error=str(e))
    # Fallback to Python
    return await py_http_request.ainvoke({"url": url, "method": method, "body": body})
