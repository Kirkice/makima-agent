"""Marketplace API routes.

Provides REST endpoints for browsing, installing, and uninstalling MCP servers
from the marketplace.
"""

from typing import Optional

from fastapi import APIRouter, Depends, HTTPException, Query
from structlog import get_logger

from makima.core.deps import get_current_user
from makima.auth.models import User
from makima.marketplace.models import (
    MarketplaceItem,
    MarketplaceItemListResponse,
    InstallRequest,
    InstallResponse,
    UninstallRequest,
    UninstallResponse,
    InstalledItemInfo,
)
from makima.marketplace.service import MarketplaceService

logger = get_logger(__name__)

router = APIRouter(prefix="/api/marketplace", tags=["marketplace"])

# Global service instance (initialized on app startup)
_marketplace_service: Optional[MarketplaceService] = None


def get_marketplace_service() -> MarketplaceService:
    """Get the marketplace service instance."""
    global _marketplace_service
    if _marketplace_service is None:
        _marketplace_service = MarketplaceService()
    return _marketplace_service


@router.get("/items", response_model=MarketplaceItemListResponse)
async def list_marketplace_items(
    search: Optional[str] = Query(None, description="Search query"),
    tags: Optional[list[str]] = Query(None, description="Filter by tags"),
    user: User = Depends(get_current_user),
) -> MarketplaceItemListResponse:
    """List all marketplace items with optional filtering.

    Args:
        search: Optional search query to filter items.
        tags: Optional list of tags to filter by.
        user: Current authenticated user.

    Returns:
        List of marketplace items with total count.
    """
    service = get_marketplace_service()

    try:
        items = service.get_items(search=search, tags=tags)
        return MarketplaceItemListResponse(items=items, total=len(items))
    except Exception as e:
        logger.error("Failed to list marketplace items", error=str(e))
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/items/{item_id}", response_model=MarketplaceItem)
async def get_marketplace_item(
    item_id: str,
    user: User = Depends(get_current_user),
) -> MarketplaceItem:
    """Get a single marketplace item by ID.

    Args:
        item_id: The unique item identifier.
        user: Current authenticated user.

    Returns:
        The marketplace item.

    Raises:
        HTTPException: 404 if item not found.
    """
    service = get_marketplace_service()

    item = service.get_item(item_id)
    if not item:
        raise HTTPException(status_code=404, detail=f"Marketplace item not found: {item_id}")

    return item


@router.get("/tags", response_model=list[str])
async def list_marketplace_tags(
    user: User = Depends(get_current_user),
) -> list[str]:
    """Get all unique tags from marketplace items.

    Args:
        user: Current authenticated user.

    Returns:
        Sorted list of unique tags.
    """
    service = get_marketplace_service()
    return service.get_tags()


@router.post("/install", response_model=InstallResponse)
async def install_marketplace_item(
    request: InstallRequest,
    user: User = Depends(get_current_user),
) -> InstallResponse:
    """Install an MCP server from the marketplace.

    Args:
        request: Installation request with item ID and parameters.
        user: Current authenticated user.

    Returns:
        Installation response with success status.

    Raises:
        HTTPException: 404 if item not found.
    """
    service = get_marketplace_service()

    item = service.get_item(request.item_id)
    if not item:
        raise HTTPException(status_code=404, detail=f"Marketplace item not found: {request.item_id}")

    try:
        response = service.install_item(
            item=item,
            target=request.target,
            selected_method_index=request.selected_method_index,
            parameters=request.parameters,
        )
        return response
    except Exception as e:
        logger.error("Failed to install marketplace item", item_id=request.item_id, error=str(e))
        return InstallResponse(
            success=False,
            item_id=request.item_id,
            server_name=request.item_id,
            config_path="",
            error=str(e)
        )


@router.post("/uninstall", response_model=UninstallResponse)
async def uninstall_marketplace_item(
    request: UninstallRequest,
    user: User = Depends(get_current_user),
) -> UninstallResponse:
    """Uninstall an MCP server.

    Args:
        request: Uninstallation request with item ID.
        user: Current authenticated user.

    Returns:
        Uninstallation response with success status.
    """
    service = get_marketplace_service()

    item = service.get_item(request.item_id)
    if not item:
        # Item might have been removed from marketplace, but we can still uninstall
        # by creating a minimal item
        from makima.marketplace.models import MarketplaceItem
        item = MarketplaceItem(
            id=request.item_id,
            name=request.item_id,
            description="",
            content="{}",
        )

    try:
        response = service.uninstall_item(item=item, target=request.target)
        return response
    except Exception as e:
        logger.error("Failed to uninstall marketplace item", item_id=request.item_id, error=str(e))
        return UninstallResponse(
            success=False,
            item_id=request.item_id,
            server_name=request.item_id,
            config_path="",
            error=str(e)
        )


@router.get("/installed", response_model=list[InstalledItemInfo])
async def list_installed_items(
    target: str = Query("global", description="Installation target: 'project' or 'global'"),
    user: User = Depends(get_current_user),
) -> list[InstalledItemInfo]:
    """Get list of installed MCP servers.

    Args:
        target: Installation target to check.
        user: Current authenticated user.

    Returns:
        List of installed item information.
    """
    service = get_marketplace_service()
    return service.get_installed_items(target=target)


@router.get("/items/{item_id}/installed")
async def check_item_installed(
    item_id: str,
    target: str = Query("global", description="Installation target to check"),
    user: User = Depends(get_current_user),
) -> dict:
    """Check if a specific marketplace item is installed.

    Args:
        item_id: The item ID to check.
        target: Installation target to check.
        user: Current authenticated user.

    Returns:
        Dictionary with 'installed' boolean.
    """
    service = get_marketplace_service()
    is_installed = service.is_installed(item_id=item_id, target=target)
    return {"installed": is_installed, "item_id": item_id, "target": target}