"""Marketplace service layer.

Orchestrates marketplace operations by combining loader and installer functionality.
"""

from typing import Optional

from structlog import get_logger

from makima.marketplace.loader import MarketplaceLoader
from makima.marketplace.installer import MarketplaceInstaller
from makima.marketplace.models import (
    MarketplaceItem,
    InstallResponse,
    UninstallResponse,
    InstalledItemInfo,
)

logger = get_logger(__name__)


class MarketplaceService:
    """High-level service for marketplace operations."""

    def __init__(
        self,
        marketplace_dir: Optional[str] = None,
        config_dir: Optional[str] = None,
    ):
        """Initialize the marketplace service.

        Args:
            marketplace_dir: Directory containing marketplace YAML files.
            config_dir: Directory for storing MCP configuration.
        """
        self.loader = MarketplaceLoader(marketplace_dir=marketplace_dir)
        self.installer = MarketplaceInstaller(config_dir=config_dir)
        logger.info("MarketplaceService initialized")

    def get_items(
        self,
        search: Optional[str] = None,
        tags: Optional[list[str]] = None,
    ) -> list[MarketplaceItem]:
        """Get marketplace items with optional filtering.

        Args:
            search: Search query to filter items.
            tags: Tags to filter by.

        Returns:
            List of marketplace items.
        """
        return self.loader.filter_items(search=search, tags=tags)

    def get_item(self, item_id: str) -> Optional[MarketplaceItem]:
        """Get a single marketplace item by ID.

        Args:
            item_id: The unique item identifier.

        Returns:
            MarketplaceItem if found, None otherwise.
        """
        return self.loader.get_item_by_id(item_id)

    def get_tags(self) -> list[str]:
        """Get all unique tags from marketplace items.

        Returns:
            Sorted list of unique tags.
        """
        return self.loader.get_all_tags()

    def install_item(
        self,
        item: MarketplaceItem,
        target: str = "global",
        selected_method_index: Optional[int] = None,
        parameters: Optional[dict[str, str]] = None,
    ) -> InstallResponse:
        """Install an MCP server from the marketplace.

        Args:
            item: The marketplace item to install.
            target: Installation target ('project' or 'global').
            selected_method_index: Index of selected installation method.
            parameters: Parameter values for template substitution.

        Returns:
            InstallResponse with success status.
        """
        return self.installer.install(
            item=item,
            target=target,
            selected_method_index=selected_method_index,
            parameters=parameters,
        )

    def uninstall_item(
        self,
        item: MarketplaceItem,
        target: str = "global",
    ) -> UninstallResponse:
        """Uninstall an MCP server.

        Args:
            item: The marketplace item to uninstall.
            target: Installation target ('project' or 'global').

        Returns:
            UninstallResponse with success status.
        """
        return self.installer.uninstall(item=item, target=target)

    def get_installed_items(self, target: str = "global") -> list[InstalledItemInfo]:
        """Get list of installed MCP servers.

        Args:
            target: Installation target to check.

        Returns:
            List of installed item information.
        """
        return self.installer.get_installed_items(target=target)

    def is_installed(self, item_id: str, target: str = "global") -> bool:
        """Check if an MCP server is installed.

        Args:
            item_id: The item ID to check.
            target: Installation target to check.

        Returns:
            True if installed, False otherwise.
        """
        return self.installer.is_installed(item_id=item_id, target=target)

    def get_items_with_install_status(
        self,
        target: str = "global",
        search: Optional[str] = None,
        tags: Optional[list[str]] = None,
    ) -> list[dict]:
        """Get marketplace items with their installation status.

        Args:
            target: Installation target to check.
            search: Search query to filter items.
            tags: Tags to filter by.

        Returns:
            List of items with 'installed' field added.
        """
        items = self.get_items(search=search, tags=tags)
        installed_ids = {i.item_id for i in self.get_installed_items(target=target)}

        result = []
        for item in items:
            item_dict = item.model_dump()
            item_dict["installed"] = item.id in installed_ids
            result.append(item_dict)

        return result