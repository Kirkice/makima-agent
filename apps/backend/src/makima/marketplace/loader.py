"""Marketplace configuration loader.

Loads MCP server definitions from the mcps.yml file in the assets directory.
"""

import os
from pathlib import Path
from typing import Optional

import yaml
from structlog import get_logger

from makima.marketplace.models import (
    MarketplaceItem,
    McpInstallationMethod,
    McpParameter,
)

logger = get_logger(__name__)


class MarketplaceLoader:
    """Loads marketplace items from YAML configuration files."""

    def __init__(self, marketplace_dir: Optional[str] = None):
        """Initialize the loader.

        Args:
            marketplace_dir: Directory containing marketplace YAML files.
                           Defaults to apps/backend/assets/marketplace/
        """
        if marketplace_dir is None:
            # Default to the assets directory relative to this file
            backend_dir = Path(__file__).parent.parent.parent.parent  # src/makima/marketplace -> apps/backend
            marketplace_dir = str(backend_dir / "assets" / "marketplace")

        self.marketplace_dir = Path(marketplace_dir)
        self._items_cache: Optional[list[MarketplaceItem]] = None
        logger.info("MarketplaceLoader initialized", marketplace_dir=str(self.marketplace_dir))

    def _get_mcps_file_path(self) -> Path:
        """Get the path to the mcps.yml file."""
        return self.marketplace_dir / "mcps.yml"

    def load_all_items(self) -> list[MarketplaceItem]:
        """Load all marketplace items from mcps.yml.

        Returns:
            List of MarketplaceItem objects.
        """
        if self._items_cache is not None:
            return self._items_cache

        mcps_file = self._get_mcps_file_path()

        if not mcps_file.exists():
            logger.warning("Marketplace config file not found", path=str(mcps_file))
            return []

        try:
            with open(mcps_file, "r", encoding="utf-8") as f:
                data = yaml.safe_load(f)

            if not data or "items" not in data:
                logger.warning("Invalid marketplace config format", path=str(mcps_file))
                return []

            items = []
            for item_data in data["items"]:
                try:
                    item = self._parse_item(item_data)
                    items.append(item)
                except Exception as e:
                    logger.error(
                        "Failed to parse marketplace item",
                        item_id=item_data.get("id", "unknown"),
                        error=str(e)
                    )
                    continue

            self._items_cache = items
            logger.info("Loaded marketplace items", count=len(items))
            return items

        except yaml.YAMLError as e:
            logger.error("Failed to parse marketplace YAML", error=str(e))
            return []
        except Exception as e:
            logger.error("Failed to load marketplace items", error=str(e))
            return []

    def _parse_item(self, data: dict) -> MarketplaceItem:
        """Parse a single marketplace item from YAML data."""
        # Parse content - can be a single string or array of methods
        content = data.get("content", "")
        parsed_content: str | list[McpInstallationMethod]

        if isinstance(content, str):
            # Single installation method (JSON string)
            parsed_content = content
        elif isinstance(content, list):
            # Multiple installation methods
            methods = []
            for method_data in content:
                method_params = [
                    McpParameter(**p) for p in method_data.get("parameters", [])
                ]
                method = McpInstallationMethod(
                    name=method_data.get("name", "Default"),
                    content=method_data.get("content", ""),
                    prerequisites=method_data.get("prerequisites", []),
                    parameters=method_params,
                )
                methods.append(method)
            parsed_content = methods
        else:
            parsed_content = ""

        # Parse global parameters
        global_params = [
            McpParameter(**p) for p in data.get("parameters", [])
        ]

        return MarketplaceItem(
            id=data["id"],
            name=data["name"],
            description=data.get("description", ""),
            author=data.get("author"),
            author_url=data.get("author_url"),
            url=data.get("url"),
            tags=data.get("tags", []),
            prerequisites=data.get("prerequisites", []),
            content=parsed_content,
            parameters=global_params,
        )

    def get_item_by_id(self, item_id: str) -> Optional[MarketplaceItem]:
        """Get a single marketplace item by ID.

        Args:
            item_id: The unique item identifier.

        Returns:
            MarketplaceItem if found, None otherwise.
        """
        items = self.load_all_items()
        for item in items:
            if item.id == item_id:
                return item
        return None

    def filter_items(
        self,
        search: Optional[str] = None,
        tags: Optional[list[str]] = None,
    ) -> list[MarketplaceItem]:
        """Filter marketplace items by search query and/or tags.

        Args:
            search: Search query to match against name and description.
            tags: List of tags to filter by (OR logic - item must have at least one).

        Returns:
            Filtered list of MarketplaceItem objects.
        """
        items = self.load_all_items()

        if not search and not tags:
            return items

        filtered = []
        search_lower = search.lower() if search else None

        for item in items:
            # Search filter
            if search_lower:
                searchable = f"{item.name} {item.description}".lower()
                if search_lower not in searchable:
                    continue

            # Tags filter (OR logic)
            if tags:
                if not any(tag in item.tags for tag in tags):
                    continue

            filtered.append(item)

        return filtered

    def get_all_tags(self) -> list[str]:
        """Get all unique tags from marketplace items.

        Returns:
            Sorted list of unique tags.
        """
        items = self.load_all_items()
        tags = set()
        for item in items:
            tags.update(item.tags)
        return sorted(tags)

    def invalidate_cache(self) -> None:
        """Clear the items cache to force reload on next access."""
        self._items_cache = None