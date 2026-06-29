"""Marketplace installer service.

Handles installation and uninstallation of MCP servers to/from mcp.json config files.
"""

import json
import os
import re
from pathlib import Path
from typing import Optional

from structlog import get_logger

from makima.marketplace.models import (
    MarketplaceItem,
    McpInstallationMethod,
    InstallResponse,
    UninstallResponse,
    InstalledItemInfo,
)

logger = get_logger(__name__)


class MarketplaceInstaller:
    """Handles MCP server installation and uninstallation."""

    def __init__(self, config_dir: Optional[str] = None):
        """Initialize the installer.

        Args:
            config_dir: Directory for storing MCP configuration.
                       Defaults to .makima/ in user's home directory.
        """
        if config_dir is None:
            # Default to ~/.makima/ for global config
            config_dir = str(Path.home() / ".makima")

        self.config_dir = Path(config_dir)
        self.config_dir.mkdir(parents=True, exist_ok=True)
        logger.info("MarketplaceInstaller initialized", config_dir=str(self.config_dir))

    def _get_mcp_config_path(self, target: str = "global") -> Path:
        """Get the path to the mcp.json config file.

        Args:
            target: 'global' for user-wide config, 'project' for workspace config.

        Returns:
            Path to the mcp.json file.
        """
        if target == "project":
            # For project-level, use .makima/mcp.json in current working directory
            return Path.cwd() / ".makima" / "mcp.json"
        else:
            # For global, use ~/.makima/mcp.json
            return self.config_dir / "mcp.json"

    def _load_mcp_config(self, config_path: Path) -> dict:
        """Load existing MCP configuration or return default structure.

        Args:
            config_path: Path to the mcp.json file.

        Returns:
            Parsed JSON config or default structure.
        """
        if not config_path.exists():
            return {"mcpServers": {}}

        try:
            with open(config_path, "r", encoding="utf-8") as f:
                data = json.load(f)
                if not isinstance(data, dict):
                    return {"mcpServers": {}}
                if "mcpServers" not in data:
                    data["mcpServers"] = {}
                return data
        except json.JSONDecodeError as e:
            logger.error("Failed to parse mcp.json", path=str(config_path), error=str(e))
            raise ValueError(f"Invalid JSON in {config_path}: {e}")
        except Exception as e:
            logger.error("Failed to read mcp.json", path=str(config_path), error=str(e))
            raise

    def _save_mcp_config(self, config_path: Path, data: dict) -> None:
        """Save MCP configuration to file.

        Args:
            config_path: Path to the mcp.json file.
            data: Configuration data to save.
        """
        config_path.parent.mkdir(parents=True, exist_ok=True)
        with open(config_path, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)
        logger.info("Saved MCP config", path=str(config_path))

    def _get_content_for_method(
        self,
        item: MarketplaceItem,
        selected_method_index: Optional[int] = None,
    ) -> tuple[str, list]:
        """Get the content template and parameters for the selected installation method.

        Args:
            item: The marketplace item.
            selected_method_index: Index of the selected method (for items with multiple methods).

        Returns:
            Tuple of (content_template, method_parameters).
        """
        if isinstance(item.content, str):
            # Single method - content is already a JSON string
            return item.content, []

        # Multiple methods - select by index
        index = selected_method_index if selected_method_index is not None else 0
        if index < 0 or index >= len(item.content):
            index = 0

        method = item.content[index]
        return method.content, method.parameters

    def _apply_parameters(self, template: str, parameters: dict[str, str]) -> str:
        """Replace {{PARAMETER_KEY}} placeholders with actual values.

        Args:
            template: Content template with placeholders.
            parameters: Dictionary of parameter values keyed by parameter key.

        Returns:
            Template with placeholders replaced.
        """
        result = template
        for key, value in parameters.items():
            # Replace {{KEY}} with value
            pattern = re.compile(r"\{\{" + re.escape(key) + r"\}\}")
            result = pattern.sub(str(value), result)
        return result

    def install(
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
            InstallResponse with success status and details.
        """
        parameters = parameters or {}

        try:
            # Get content template and merge parameters
            content_template, method_params = self._get_content_for_method(item, selected_method_index)

            # Merge global and method-specific parameters
            all_params = {p.key: p for p in item.parameters}
            for p in method_params:
                all_params[p.key] = p

            # Validate required parameters
            for param in all_params.values():
                if not param.optional and param.key not in parameters:
                    return InstallResponse(
                        success=False,
                        item_id=item.id,
                        server_name=item.id,
                        config_path="",
                        error=f"Missing required parameter: {param.name}"
                    )

            # Apply parameters to template
            final_content = self._apply_parameters(content_template, parameters)

            # Parse the MCP server config
            try:
                server_config = json.loads(final_content)
            except json.JSONDecodeError as e:
                return InstallResponse(
                    success=False,
                    item_id=item.id,
                    server_name=item.id,
                    config_path="",
                    error=f"Invalid JSON in installation template: {e}"
                )

            # Load existing config
            config_path = self._get_mcp_config_path(target)
            existing_config = self._load_mcp_config(config_path)

            # Add/update the server
            server_name = item.id
            existing_config["mcpServers"][server_name] = server_config

            # Save config
            self._save_mcp_config(config_path, existing_config)

            # Find line number where server was added (for UI feedback)
            line = None
            try:
                with open(config_path, "r", encoding="utf-8") as f:
                    lines = f.readlines()
                    for i, line_content in enumerate(lines, 1):
                        if f'"{server_name}"' in line_content:
                            line = i
                            break
            except Exception:
                pass

            logger.info(
                "Installed MCP server",
                item_id=item.id,
                server_name=server_name,
                target=target,
                config_path=str(config_path),
            )

            return InstallResponse(
                success=True,
                item_id=item.id,
                server_name=server_name,
                config_path=str(config_path),
                line=line,
            )

        except Exception as e:
            logger.error("Failed to install MCP server", item_id=item.id, error=str(e))
            return InstallResponse(
                success=False,
                item_id=item.id,
                server_name=item.id,
                config_path="",
                error=str(e)
            )

    def uninstall(
        self,
        item: MarketplaceItem,
        target: str = "global",
    ) -> UninstallResponse:
        """Uninstall an MCP server.

        Args:
            item: The marketplace item to uninstall.
            target: Installation target ('project' or 'global').

        Returns:
            UninstallResponse with success status and details.
        """
        try:
            config_path = self._get_mcp_config_path(target)

            if not config_path.exists():
                return UninstallResponse(
                    success=True,
                    item_id=item.id,
                    server_name=item.id,
                    config_path=str(config_path),
                    error=None,
                )

            existing_config = self._load_mcp_config(config_path)
            server_name = item.id

            if server_name in existing_config.get("mcpServers", {}):
                del existing_config["mcpServers"][server_name]
                self._save_mcp_config(config_path, existing_config)
                logger.info(
                    "Uninstalled MCP server",
                    item_id=item.id,
                    server_name=server_name,
                    target=target,
                )

            return UninstallResponse(
                success=True,
                item_id=item.id,
                server_name=server_name,
                config_path=str(config_path),
            )

        except Exception as e:
            logger.error("Failed to uninstall MCP server", item_id=item.id, error=str(e))
            return UninstallResponse(
                success=False,
                item_id=item.id,
                server_name=item.id,
                config_path=str(self._get_mcp_config_path(target)),
                error=str(e)
            )

    def get_installed_items(self, target: str = "global") -> list[InstalledItemInfo]:
        """Get list of installed MCP servers.

        Args:
            target: Installation target to check ('project' or 'global').

        Returns:
            List of InstalledItemInfo for each installed server.
        """
        config_path = self._get_mcp_config_path(target)

        if not config_path.exists():
            return []

        try:
            config = self._load_mcp_config(config_path)
            servers = config.get("mcpServers", {})

            installed = []
            for server_name, server_config in servers.items():
                installed.append(InstalledItemInfo(
                    item_id=server_name,
                    server_name=server_name,
                    target=target,
                    config_path=str(config_path),
                    enabled=server_config.get("enabled", True),
                ))

            return installed

        except Exception as e:
            logger.error("Failed to get installed items", error=str(e))
            return []

    def is_installed(self, item_id: str, target: str = "global") -> bool:
        """Check if an MCP server is installed.

        Args:
            item_id: The item ID to check.
            target: Installation target to check.

        Returns:
            True if installed, False otherwise.
        """
        config_path = self._get_mcp_config_path(target)

        if not config_path.exists():
            return False

        try:
            config = self._load_mcp_config(config_path)
            return item_id in config.get("mcpServers", {})
        except Exception:
            return False