"""Configuration center service for dynamic settings management."""

from __future__ import annotations

import json
from datetime import datetime, timezone
from typing import Any

import redis.asyncio as aioredis

from makima_common.config import get_settings
from makima_common.logging import get_logger

logger = get_logger(__name__)


class ConfigCenter:
    """Redis-backed configuration center for dynamic settings."""

    def __init__(self, redis_url: str | None = None) -> None:
        settings = get_settings()
        self.redis_url = redis_url or settings.redis_url
        self._redis: aioredis.Redis | None = None
        self._cache: dict[str, Any] = {}
        self._cache_ttl = 60  # seconds

    async def initialize(self) -> None:
        """Initialize Redis connection."""
        try:
            self._redis = aioredis.from_url(self.redis_url, decode_responses=True)
            await self._redis.ping()
            logger.info("Config center initialized", redis_url=self.redis_url)
        except Exception as e:
            logger.warning("Config center Redis connection failed", error=str(e))
            self._redis = None

    async def close(self) -> None:
        """Close Redis connection."""
        if self._redis:
            await self._redis.close()
            self._redis = None

    async def get(self, key: str, default: Any = None) -> Any:
        """Get a configuration value.

        Args:
            key: Configuration key.
            default: Default value if not found.

        Returns:
            Configuration value.
        """
        # Check cache first
        cache_key = f"config:{key}"
        if cache_key in self._cache:
            cached_value, cached_time = self._cache[cache_key]
            if (datetime.now(timezone.utc).timestamp() - cached_time) < self._cache_ttl:
                return cached_value

        # Try Redis
        if self._redis:
            try:
                value = await self._redis.get(f"config:{key}")
                if value is not None:
                    parsed = json.loads(value)
                    self._cache[cache_key] = (parsed, datetime.now(timezone.utc).timestamp())
                    return parsed
            except Exception as e:
                logger.warning("Config center get failed", key=key, error=str(e))

        return default

    async def set(self, key: str, value: Any, ttl: int | None = None) -> bool:
        """Set a configuration value.

        Args:
            key: Configuration key.
            value: Configuration value.
            ttl: Optional TTL in seconds.

        Returns:
            True if successful.
        """
        if not self._redis:
            logger.warning("Config center not available", key=key)
            return False

        try:
            serialized = json.dumps(value)
            redis_key = f"config:{key}"

            if ttl:
                await self._redis.setex(redis_key, ttl, serialized)
            else:
                await self._redis.set(redis_key, serialized)

            # Update cache
            cache_key = f"config:{key}"
            self._cache[cache_key] = (value, datetime.now(timezone.utc).timestamp())

            logger.info("Config updated", key=key)
            return True
        except Exception as e:
            logger.error("Config center set failed", key=key, error=str(e))
            return False

    async def delete(self, key: str) -> bool:
        """Delete a configuration value.

        Args:
            key: Configuration key.

        Returns:
            True if deleted.
        """
        if not self._redis:
            return False

        try:
            redis_key = f"config:{key}"
            result = await self._redis.delete(redis_key)

            # Remove from cache
            cache_key = f"config:{key}"
            self._cache.pop(cache_key, None)

            return result > 0
        except Exception as e:
            logger.error("Config center delete failed", key=key, error=str(e))
            return False

    async def list_keys(self, pattern: str = "*") -> list[str]:
        """List all configuration keys matching pattern.

        Args:
            pattern: Redis key pattern.

        Returns:
            List of keys (without config: prefix).
        """
        if not self._redis:
            return []

        try:
            keys = await self._redis.keys(f"config:{pattern}")
            return [k.replace("config:", "", 1) for k in keys]
        except Exception as e:
            logger.error("Config center list failed", error=str(e))
            return []

    async def get_all(self) -> dict[str, Any]:
        """Get all configuration values.

        Returns:
            Dictionary of all config key-value pairs.
        """
        keys = await self.list_keys()
        result = {}
        for key in keys:
            value = await self.get(key)
            if value is not None:
                result[key] = value
        return result

    def clear_cache(self) -> None:
        """Clear the local cache."""
        self._cache.clear()


# Global instance
config_center = ConfigCenter()