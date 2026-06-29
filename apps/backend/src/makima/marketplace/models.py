"""Pydantic models for MCP Marketplace.

Defines the data structures for marketplace items, installation methods,
parameters, and API request/response models.
"""

from typing import Optional
from pydantic import BaseModel, Field


class McpParameter(BaseModel):
    """A parameter required for MCP server installation."""
    name: str = Field(..., description="Human-readable parameter name")
    key: str = Field(..., description="Placeholder key used in content template (e.g., API_KEY)")
    placeholder: str = Field(default="", description="Example value to show in UI")
    optional: bool = Field(default=False, description="Whether this parameter is optional")


class McpInstallationMethod(BaseModel):
    """A specific installation method for an MCP server."""
    name: str = Field(..., description="Method name (e.g., 'NPX', 'Docker', 'Remote MCP')")
    content: str = Field(..., description="JSON template for mcp.json configuration")
    prerequisites: list[str] = Field(default_factory=list, description="Required prerequisites")
    parameters: list[McpParameter] = Field(default_factory=list, description="Method-specific parameters")


class MarketplaceItem(BaseModel):
    """An item in the MCP marketplace."""
    id: str = Field(..., description="Unique identifier")
    name: str = Field(..., description="Display name")
    description: str = Field(..., description="Detailed description")
    author: Optional[str] = Field(default=None, description="Author/organization name")
    author_url: Optional[str] = Field(default=None, description="Author's URL")
    url: Optional[str] = Field(default=None, description="GitHub/project URL")
    tags: list[str] = Field(default_factory=list, description="Categorization tags")
    prerequisites: list[str] = Field(default_factory=list, description="Global prerequisites")
    content: str | list[McpInstallationMethod] = Field(
        ...,
        description="Installation configuration - single JSON string or array of methods"
    )
    parameters: list[McpParameter] = Field(default_factory=list, description="Global parameters")


class MarketplaceItemListResponse(BaseModel):
    """Response model for listing marketplace items."""
    items: list[MarketplaceItem]
    total: int


class InstallRequest(BaseModel):
    """Request model for installing an MCP server."""
    item_id: str = Field(..., description="Marketplace item ID to install")
    target: str = Field(default="global", description="Installation target: 'project' or 'global'")
    selected_method_index: Optional[int] = Field(
        default=None,
        description="Index of selected installation method (for items with multiple methods)"
    )
    parameters: dict[str, str] = Field(
        default_factory=dict,
        description="Parameter values keyed by parameter key"
    )


class InstallResponse(BaseModel):
    """Response model after installation."""
    success: bool
    item_id: str
    server_name: str
    config_path: str = Field(..., description="Path to the mcp.json file that was modified")
    line: Optional[int] = Field(default=None, description="Line number where server was added")
    error: Optional[str] = None


class UninstallRequest(BaseModel):
    """Request model for uninstalling an MCP server."""
    item_id: str = Field(..., description="Marketplace item ID to uninstall")
    target: str = Field(default="global", description="Installation target: 'project' or 'global'")


class UninstallResponse(BaseModel):
    """Response model after uninstallation."""
    success: bool
    item_id: str
    server_name: str
    config_path: str
    error: Optional[str] = None


class InstalledItemInfo(BaseModel):
    """Information about an installed MCP server."""
    item_id: str
    server_name: str
    target: str  # "project" or "global"
    config_path: str
    enabled: bool = True