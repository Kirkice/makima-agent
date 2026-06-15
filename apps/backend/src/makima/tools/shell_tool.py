"""Shell command execution tool."""

from __future__ import annotations

import asyncio
import os
from pathlib import Path

from langchain_core.tools import tool

from makima_common.config import get_settings
from makima_common.logging import get_logger

logger = get_logger(__name__)


@tool
async def execute_shell(command: str) -> str:
    """Execute a shell command in the sandbox directory.

    Args:
        command: The shell command to execute.

    Returns:
        Combined stdout and stderr output, or error message.
    """
    settings = get_settings()
    working_dir = Path(settings.tool_working_dir)
    working_dir.mkdir(parents=True, exist_ok=True)

    # Block dangerous commands
    dangerous_patterns = ["rm -rf /", "mkfs", "dd if=", ":(){", "fork bomb", "> /dev/sda"]
    command_lower = command.lower()
    for pattern in dangerous_patterns:
        if pattern in command_lower:
            return f"Error: Command blocked for safety: {command}"

    logger.info("Executing shell command", command=command, cwd=str(working_dir))

    try:
        process = await asyncio.create_subprocess_shell(
            command,
            cwd=str(working_dir),
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )

        try:
            stdout, stderr = await asyncio.wait_for(
                process.communicate(),
                timeout=settings.tool_timeout,
            )
        except asyncio.TimeoutError:
            process.kill()
            return f"Error: Command timed out after {settings.tool_timeout} seconds"

        output_parts = []
        if stdout:
            decoded = stdout.decode("utf-8", errors="replace")
            output_parts.append(decoded)
        if stderr:
            decoded = stderr.decode("utf-8", errors="replace")
            output_parts.append(f"[stderr]\n{decoded}")

        if process.returncode != 0:
            output_parts.append(f"\n[exit code: {process.returncode}]")

        result = "\n".join(output_parts) if output_parts else "(no output)"
        return result

    except Exception as e:
        return f"Error executing command: {e}"