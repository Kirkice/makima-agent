"""Backward-compatible re-exports from auth.service.

DEPRECATED: Import from `makima.auth.service` directly.
"""

from __future__ import annotations

from makima.auth.service import (
    ALGORITHM,
    ACCESS_TOKEN_EXPIRE_MINUTES,
    create_access_token,
    decode_access_token,
    get_current_user_id,
    hash_password,
    verify_password,
)

__all__ = [
    "ALGORITHM",
    "ACCESS_TOKEN_EXPIRE_MINUTES",
    "create_access_token",
    "decode_access_token",
    "get_current_user_id",
    "hash_password",
    "verify_password",
]
