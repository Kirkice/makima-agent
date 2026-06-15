"""HTTP request tools."""

from __future__ import annotations

import json
from typing import Literal

import httpx
from langchain_core.tools import tool

from makima_common.config import get_settings
from makima_common.logging import get_logger

logger = get_logger(__name__)


@tool
async def http_request(
    url: str,
    method: Literal["GET", "POST", "PUT", "DELETE", "PATCH"] = "GET",
    headers: str = "",
    body: str = "",
) -> str:
    """Send an HTTP request and return the response.

    Args:
        url: The URL to request.
        method: HTTP method (GET, POST, PUT, DELETE, PATCH).
        headers: JSON string of headers, e.g. '{"Content-Type": "application/json"}'.
        body: Request body as a string.

    Returns:
        Response status code and body.
    """
    settings = get_settings()

    # Block requests to internal networks
    blocked_prefixes = ["http://localhost", "http://127.0.0.1", "http://10.", "http://192.168."]
    if any(url.startswith(prefix) for prefix in blocked_prefixes):
        return f"Error: Requests to internal networks are blocked"

    parsed_headers = {}
    if headers:
        try:
            parsed_headers = json.loads(headers)
        except json.JSONDecodeError:
            return "Error: Invalid headers JSON"

    try:
        async with httpx.AsyncClient(timeout=settings.tool_timeout) as client:
            response = await client.request(
                method=method,
                url=url,
                headers=parsed_headers,
                content=body if body else None,
            )

        # Truncate large responses
        response_text = response.text
        max_length = 4000
        if len(response_text) > max_length:
            response_text = response_text[:max_length] + "\n... (truncated)"

        return f"Status: {response.status_code}\n\n{response_text}"

    except httpx.TimeoutException:
        return f"Error: Request timed out after {settings.tool_timeout} seconds"
    except Exception as e:
        return f"Error making HTTP request: {e}"