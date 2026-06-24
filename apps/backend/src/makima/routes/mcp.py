"""MCP (Model Context Protocol) server management API routes.

Provides endpoints for the frontend to list, reconnect, and toggle MCP servers.
The actual MCP server lifecycle is managed by the agent's tool system at runtime.
"""

from __future__ import annotations

from typing import Any

from fastapi import APIRouter, Depends
from pydantic import BaseModel, Field

from makima.auth.models import User
from makima.core.deps import get_current_user
from makima_common.logging import get_logger

logger = get_logger(__name__)

router = APIRouter(prefix="/api/mcp", tags=["mcp"])


# ── Request / Response Models ────────────────────────────────────────


class McpToolResponse(BaseModel):
    """Single MCP tool."""

    name: str = Field(..., description="Tool name")
    description: str | None = Field(None, description="Tool description")
    enabled: bool = Field(default=True, description="Whether the tool is enabled")


class McpServerResponse(BaseModel):
    """Single MCP server status."""

    name: str = Field(..., description="Server name")
    transport_type: str | None = Field(None, description="Transport type: stdio, sse, streamable-http")
    status: str = Field(default="disconnected", description="Connection status")
    enabled: bool = Field(default=True, description="Whether the server is enabled")
    last_error: str | None = Field(None, description="Last error message")
    tools: list[McpToolResponse] = Field(default_factory=list, description="Available tools")


class McpToggleRequest(BaseModel):
    """Request to toggle an MCP server."""

    enabled: bool = Field(..., description="Enable or disable")


# ── In-memory MCP server registry ────────────────────────────────────
# In a production setup, this would be backed by a database or config file.
# For now we maintain a simple in-memory registry that can be queried by the
# frontend and populated by the agent runtime.

_mcp_servers: dict[str, dict[str, Any]] = {}


def _get_server(name: str) -> dict[str, Any] | None:
    return _mcp_servers.get(name)


def register_mcp_server(
    name: str,
    transport_type: str | None = None,
    status: str = "disconnected",
    enabled: bool = True,
    tools: list[dict[str, Any]] | None = None,
) -> None:
    """Register or update an MCP server in the in-memory registry.

    Called by the agent runtime when MCP servers are discovered or change state.
    """
    _mcp_servers[name] = {
        "name": name,
        "transport_type": transport_type,
        "status": status,
        "enabled": enabled,
        "last_error": None,
        "tools": tools or [],
    }


def get_all_mcp_servers() -> list[dict[str, Any]]:
    """Return all registered MCP servers."""
    return list(_mcp_servers.values())


# ── Endpoints ────────────────────────────────────────────────────────


@router.get("/servers", response_model=list[McpServerResponse])
async def list_mcp_servers(
    user: User = Depends(get_current_user),
) -> list[McpServerResponse]:
    """List all registered MCP servers with their status and tools."""
    servers = get_all_mcp_servers()
    return [
        McpServerResponse(
            name=s["name"],
            transport_type=s.get("transport_type"),
            status=s.get("status", "disconnected"),
            enabled=s.get("enabled", True),
            last_error=s.get("last_error"),
            tools=[
                McpToolResponse(
                    name=t.get("name", ""),
                    description=t.get("description"),
                    enabled=t.get("enabled", True),
                )
                for t in s.get("tools", [])
            ],
        )
        for s in servers
    ]


@router.post("/servers/{name}/reconnect", response_model=McpServerResponse)
async def reconnect_mcp_server(
    name: str,
    user: User = Depends(get_current_user),
) -> McpServerResponse:
    """Reconnect an MCP server.

    In the current implementation this resets the error state and sets
    status to 'connecting'. The actual reconnection happens asynchronously
    via the agent runtime.
    """
    server = _get_server(name)
    if server is None:
        # Auto-register the server if not found
        register_mcp_server(name=name, status="connecting")
        server = _get_server(name)

    # Reset error and set status to connecting
    server["last_error"] = None
    server["status"] = "connecting"

    logger.info("MCP reconnect requested", server_name=name)

    return McpServerResponse(
        name=server["name"],
        transport_type=server.get("transport_type"),
        status=server["status"],
        enabled=server.get("enabled", True),
        last_error=server.get("last_error"),
        tools=[
            McpToolResponse(
                name=t.get("name", ""),
                description=t.get("description"),
                enabled=t.get("enabled", True),
            )
            for t in server.get("tools", [])
        ],
    )


@router.post("/servers/{name}/toggle", response_model=McpServerResponse)
async def toggle_mcp_server(
    name: str,
    request: McpToggleRequest,
    user: User = Depends(get_current_user),
) -> McpServerResponse:
    """Enable or disable an MCP server."""
    server = _get_server(name)
    if server is None:
        # Auto-register the server if not found
        register_mcp_server(name=name, enabled=request.enabled)
        server = _get_server(name)
    else:
        server["enabled"] = request.enabled

    logger.info(
        "MCP server toggled",
        server_name=name,
        enabled=request.enabled,
    )

    return McpServerResponse(
        name=server["name"],
        transport_type=server.get("transport_type"),
        status=server.get("status", "disconnected"),
        enabled=server.get("enabled", True),
        last_error=server.get("last_error"),
        tools=[
            McpToolResponse(
                name=t.get("name", ""),
                description=t.get("description"),
                enabled=t.get("enabled", True),
            )
            for t in server.get("tools", [])
        ],
    )