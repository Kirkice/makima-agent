"""File system tools with three-layer access control.

Layer 1: Sandbox (tool_working_dir) - auto-approved
Layer 2: Configured whitelist (tool_allowed_dirs) - auto-approved
Layer 3: Session whitelist (user-approved paths) - auto-approved
Layer 4: Requires approval - prompts user via ApprovalManager
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import Optional

from langchain_core.tools import tool

from makima_common.config import get_settings
from makima_common.logging import get_logger
from makima.tools.path_whitelist import get_path_whitelist

logger = get_logger(__name__)


class PathAccessError(Exception):
    """Raised when path access is denied."""
    pass


class PathRequiresApproval(Exception):
    """Raised when path access requires user approval."""
    def __init__(self, path: str, reason: str):
        self.path = path
        self.reason = reason
        super().__init__(f"Path requires approval: {path} ({reason})")


def _check_path_layers(file_path: str, session_id: Optional[str] = None) -> tuple[Path, str]:
    """Check path against all access layers.
    
    Args:
        file_path: The file path to check
        session_id: Optional session ID for session whitelist
        
    Returns:
        Tuple of (resolved_path, layer_name)
        
    Raises:
        PathRequiresApproval: If path requires user approval
    """
    settings = get_settings()
    target = Path(file_path).resolve()
    
    # Layer 1: Sandbox directory (auto-approved)
    sandbox_base = Path(settings.tool_working_dir).resolve()
    if str(target).startswith(str(sandbox_base)):
        return target, "sandbox"
    
    # Layer 2: Configured whitelist (auto-approved)
    for allowed_dir in settings.tool_allowed_dirs:
        allowed_path = Path(allowed_dir).resolve()
        if str(target).startswith(str(allowed_path)):
            return target, f"whitelist:{allowed_dir}"
    
    # Layer 3: Session whitelist (auto-approved for approved paths)
    if session_id:
        whitelist = get_path_whitelist()
        if whitelist.is_path_allowed(session_id, str(target)):
            return target, "session_whitelist"
    
    # Layer 4: Requires approval
    raise PathRequiresApproval(
        path=str(target),
        reason=f"Path is outside sandbox and not in whitelist"
    )


async def _validate_path(file_path: str, session_id: Optional[str] = None, 
                        approval_manager=None) -> Path:
    """Validate path with three-layer access control and approval flow.
    
    Args:
        file_path: The file path to validate
        session_id: Optional session ID for session whitelist
        approval_manager: Optional ApprovalManager instance
        
    Returns:
        Resolved and validated Path object
        
    Raises:
        PathAccessError: If access is denied
    """
    try:
        # Try layers 1-3 first
        target, layer = _check_path_layers(file_path, session_id)
        logger.debug(f"Path approved via {layer}: {target}")
        return target
        
    except PathRequiresApproval as e:
        # Layer 4: Request approval
        if not approval_manager:
            raise PathAccessError(
                f"Access denied: {file_path} is outside sandbox and no approval manager available"
            )
        
        logger.info(f"Requesting approval for path: {file_path}")
        
        # Request approval from user
        approval = await approval_manager.request_approval(
            tool_name="file_access",
            tool_args={"path": str(e.path), "reason": e.reason, "original_path": file_path},
            risk_level="high",  # File access outside sandbox is high risk
            session_id=session_id,
        )
        
        from makima.orchestrator.approval import ApprovalStatus
        if approval.status not in (ApprovalStatus.APPROVED, ApprovalStatus.AUTO_APPROVED):
            raise PathAccessError(
                f"Access denied: User rejected access to {file_path} (status: {approval.status})"
            )
        
        # Add to session whitelist for future access
        if session_id:
            whitelist = get_path_whitelist()
            whitelist.add_path(session_id, str(e.path))
            logger.info(f"Added to session whitelist: {e.path}")
        
        return Path(e.path)


# Legacy synchronous version for backward compatibility
def _safe_path(file_path: str) -> Path:
    """Validate and return a safe path (synchronous, layers 1-2 only)."""
    settings = get_settings()
    target = Path(file_path).resolve()
    
    # Layer 1: Sandbox directory
    base = Path(settings.tool_working_dir).resolve()
    if str(target).startswith(str(base)):
        return target
    
    # Layer 2: Configured whitelist
    for allowed_dir in settings.tool_allowed_dirs:
        allowed_path = Path(allowed_dir).resolve()
        if str(target).startswith(str(allowed_path)):
            return target
    
    # Outside allowed directories
    raise PathAccessError(f"Access denied: {file_path} is outside allowed directories")


@tool
async def read_file(file_path: str, session_id: Optional[str] = None) -> str:
    """Read the contents of a file.

    Args:
        file_path: Path to the file (absolute or relative).
        session_id: Optional session ID for session whitelist.

    Returns:
        The file contents as a string.
    """
    approval_mgr = None
    try:
        from makima.orchestrator.approval import get_approval_manager
        approval_mgr = get_approval_manager()
    except Exception:
        pass
    
    try:
        target = await _validate_path(file_path, session_id, approval_mgr)
    except PathAccessError as e:
        return f"Error: {str(e)}"
    
    if not target.exists():
        return f"Error: File not found: {file_path}"
    if not target.is_file():
        return f"Error: Not a file: {file_path}"
    try:
        return target.read_text(encoding="utf-8")
    except Exception as e:
        return f"Error reading file: {e}"


@tool
async def write_file(file_path: str, content: str, session_id: Optional[str] = None) -> str:
    """Write content to a file. Creates the file if it doesn't exist.

    Args:
        file_path: Path to the file (absolute or relative).
        content: The content to write.
        session_id: Optional session ID for session whitelist.

    Returns:
        Success or error message.
    """
    approval_mgr = None
    try:
        from makima.orchestrator.approval import get_approval_manager
        approval_mgr = get_approval_manager()
    except Exception:
        pass
    
    try:
        target = await _validate_path(file_path, session_id, approval_mgr)
    except PathAccessError as e:
        return f"Error: {str(e)}"
    
    try:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")
        return f"Successfully wrote {len(content)} characters to {file_path}"
    except Exception as e:
        return f"Error writing file: {e}"


@tool
async def list_directory(dir_path: str = ".", session_id: Optional[str] = None) -> str:
    """List files and directories.

    Args:
        dir_path: Path to the directory (absolute or relative).
        session_id: Optional session ID for session whitelist.

    Returns:
        A formatted list of directory contents.
    """
    approval_mgr = None
    try:
        from makima.orchestrator.approval import get_approval_manager
        approval_mgr = get_approval_manager()
    except Exception:
        pass
    
    try:
        target = await _validate_path(dir_path, session_id, approval_mgr)
    except PathAccessError as e:
        return f"Error: {str(e)}"
    
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
