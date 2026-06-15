"""Role-Based Access Control (RBAC) implementation."""

from __future__ import annotations

from enum import Enum
from functools import wraps
from typing import Any, Callable

from fastapi import Depends, HTTPException, status
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from makima.core.db import get_async_session
from makima.auth.models import User


class Role(str, Enum):
    """User roles with hierarchical permissions."""

    ADMIN = "admin"
    USER = "user"
    VIEWER = "viewer"
    GUEST = "guest"

    @property
    def level(self) -> int:
        """Return numeric level for comparison (higher = more permissions)."""
        levels = {
            Role.ADMIN: 4,
            Role.USER: 3,
            Role.VIEWER: 2,
            Role.GUEST: 1,
        }
        return levels[self]


class Permission(str, Enum):
    """Granular permissions for resources."""

    # Sessions
    SESSION_CREATE = "session:create"
    SESSION_READ = "session:read"
    SESSION_UPDATE = "session:update"
    SESSION_DELETE = "session:delete"

    # Tasks
    TASK_CREATE = "task:create"
    TASK_READ = "task:read"
    TASK_EXECUTE = "task:execute"

    # Documents (Knowledge)
    DOCUMENT_UPLOAD = "document:upload"
    DOCUMENT_READ = "document:read"
    DOCUMENT_DELETE = "document:delete"

    # Memories
    MEMORY_READ = "memory:read"
    MEMORY_WRITE = "memory:write"
    MEMORY_DELETE = "memory:delete"

    # Admin
    ADMIN_USERS = "admin:users"
    ADMIN_SYSTEM = "admin:system"
    ADMIN_AUDIT = "admin:audit"


# Role -> Permissions mapping
ROLE_PERMISSIONS: dict[Role, set[Permission]] = {
    Role.ADMIN: set(Permission),  # All permissions
    Role.USER: {
        Permission.SESSION_CREATE,
        Permission.SESSION_READ,
        Permission.SESSION_UPDATE,
        Permission.SESSION_DELETE,
        Permission.TASK_CREATE,
        Permission.TASK_READ,
        Permission.TASK_EXECUTE,
        Permission.DOCUMENT_UPLOAD,
        Permission.DOCUMENT_READ,
        Permission.DOCUMENT_DELETE,
        Permission.MEMORY_READ,
        Permission.MEMORY_WRITE,
        Permission.MEMORY_DELETE,
    },
    Role.VIEWER: {
        Permission.SESSION_READ,
        Permission.TASK_READ,
        Permission.DOCUMENT_READ,
        Permission.MEMORY_READ,
    },
    Role.GUEST: {
        Permission.SESSION_READ,
        Permission.TASK_READ,
    },
}


def has_permission(user_role: Role, required_permission: Permission) -> bool:
    """Check if a role has a specific permission."""
    return required_permission in ROLE_PERMISSIONS.get(user_role, set())


def has_role_level(user_role: Role, min_role: Role) -> bool:
    """Check if user role meets minimum role level."""
    return user_role.level >= min_role.level


def check_permission(user: User, required_permission: Permission) -> bool:
    """Check if a user has a specific permission.
    
    Args:
        user: The user to check
        required_permission: The permission to check for
        
    Returns:
        True if the user has the permission, False otherwise
    """
    # Get user's role, default to USER if not set
    role_str = getattr(user, "role", "user")
    try:
        user_role = Role(role_str)
    except ValueError:
        user_role = Role.USER
    
    return has_permission(user_role, required_permission)


def get_current_user() -> User:
    """Placeholder for current user dependency."""
    # This will be implemented with proper JWT auth
    pass


async def get_current_user_role(
    db: AsyncSession = Depends(get_async_session),
    current_user: User = Depends(get_current_user),
) -> Role:
    """Get the current user's role."""
    # User model should have a role field; default to USER if not set
    role_str = getattr(current_user, "role", "user")
    try:
        return Role(role_str)
    except ValueError:
        return Role.USER


def require_permission(permission: Permission) -> Callable:
    """Decorator to require a specific permission for an endpoint."""

    def decorator(func: Callable) -> Callable:
        @wraps(func)
        async def wrapper(*args: Any, **kwargs: Any) -> Any:
            # Extract user from kwargs (FastAPI dependency injection)
            current_user = kwargs.get("current_user")
            if not current_user:
                raise HTTPException(
                    status_code=status.HTTP_401_UNAUTHORIZED,
                    detail="Not authenticated",
                )

            role_str = getattr(current_user, "role", "user")
            try:
                user_role = Role(role_str)
            except ValueError:
                user_role = Role.USER

            if not has_permission(user_role, permission):
                raise HTTPException(
                    status_code=status.HTTP_403_FORBIDDEN,
                    detail=f"Permission denied: {permission.value}",
                )

            return await func(*args, **kwargs)

        return wrapper

    return decorator


def require_role(min_role: Role) -> Callable:
    """Decorator to require a minimum role level."""

    def decorator(func: Callable) -> Callable:
        @wraps(func)
        async def wrapper(*args: Any, **kwargs: Any) -> Any:
            current_user = kwargs.get("current_user")
            if not current_user:
                raise HTTPException(
                    status_code=status.HTTP_401_UNAUTHORIZED,
                    detail="Not authenticated",
                )

            role_str = getattr(current_user, "role", "user")
            try:
                user_role = Role(role_str)
            except ValueError:
                user_role = Role.USER

            if not has_role_level(user_role, min_role):
                raise HTTPException(
                    status_code=status.HTTP_403_FORBIDDEN,
                    detail=f"Insufficient role: requires {min_role.value} or higher",
                )

            return await func(*args, **kwargs)

        return wrapper

    return decorator


class RBACMiddleware:
    """FastAPI middleware for RBAC enforcement."""

    def __init__(self, app: Any) -> None:
        self.app = app

    async def __call__(self, scope: dict, receive: Callable, send: Callable) -> None:
        """Process request with RBAC checks."""
        # For now, pass through - actual enforcement happens at route level
        await self.app(scope, receive, send)