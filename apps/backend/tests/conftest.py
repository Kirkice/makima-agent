"""Pytest configuration and fixtures."""

from __future__ import annotations

import pytest
from unittest.mock import patch


@pytest.fixture(autouse=True)
def mock_settings():
    """Mock settings for tests."""
    with patch("makima_common.config.get_settings") as mock:
        mock.return_value.database_url = "sqlite+aiosqlite:///:memory:"
        mock.return_value.llm_api_key = "test-key"
        mock.return_value.tool_working_dir = "/tmp/test-sandbox"
        mock.return_value.tool_timeout = 5
        mock.return_value.api_cors_origins = ["*"]
        mock.return_value.app_name = "makima-test"
        mock.return_value.debug = False
        yield mock