"""Mode management API endpoints."""

from __future__ import annotations

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

from makima.modes import get_mode, list_modes, register_mode, unregister_mode, has_mode
from makima.modes.loader import load_project_modes
from makima_schemas import ModeConfig, ToolGroupConfig

router = APIRouter(prefix="/api/modes", tags=["modes"])


class ModeListResponse(BaseModel):
    """Response for listing all modes."""

    modes: list[ModeConfig]
    total: int


class ModeResponse(BaseModel):
    """Response for a single mode."""

    mode: ModeConfig


class ModeCreateRequest(BaseModel):
    """Request body for creating a custom mode."""

    slug: str = Field(..., description="Unique mode identifier")
    name: str = Field(..., description="Display name")
    role_definition: str = Field(..., description="Role definition for system prompt")
    when_to_use: str | None = Field(None, description="When to use this mode")
    description: str | None = Field(None, description="Short description")
    custom_instructions: str | None = Field(None, description="Additional instructions")
    tool_groups: list[ToolGroupConfig] = Field(default_factory=list, description="Available tool groups")
    max_steps: int = Field(30, ge=1, le=100, description="Maximum execution steps")
    temperature: float = Field(0.0, ge=0.0, le=2.0, description="LLM temperature")


class ModeDeleteResponse(BaseModel):
    """Response for deleting a mode."""

    deleted: bool
    slug: str


@router.get("", response_model=ModeListResponse)
async def get_modes() -> ModeListResponse:
    """List all available modes."""
    modes = list_modes()
    return ModeListResponse(modes=modes, total=len(modes))


@router.get("/{slug}", response_model=ModeResponse)
async def get_mode_by_slug(slug: str) -> ModeResponse:
    """Get a specific mode by its slug."""
    mode = get_mode(slug)
    if mode is None:
        raise HTTPException(status_code=404, detail=f"Mode '{slug}' not found")
    return ModeResponse(mode=mode)


@router.post("", response_model=ModeResponse, status_code=201)
async def create_mode(request: ModeCreateRequest) -> ModeResponse:
    """Create a new custom mode."""
    # Check if mode already exists
    if has_mode(request.slug):
        raise HTTPException(status_code=409, detail=f"Mode '{request.slug}' already exists")

    # Create ModeConfig
    mode = ModeConfig(
        slug=request.slug,
        name=request.name,
        role_definition=request.role_definition,
        when_to_use=request.when_to_use,
        description=request.description,
        custom_instructions=request.custom_instructions,
        tool_groups=request.tool_groups,
        max_steps=request.max_steps,
        temperature=request.temperature,
        source="custom",
    )

    # Register the mode
    register_mode(mode)

    return ModeResponse(mode=mode)


@router.delete("/{slug}", response_model=ModeDeleteResponse)
async def delete_mode(slug: str) -> ModeDeleteResponse:
    """Delete a custom mode."""
    mode = get_mode(slug)
    if mode is None:
        raise HTTPException(status_code=404, detail=f"Mode '{slug}' not found")

    # Prevent deleting built-in modes
    if mode.source == "builtin":
        raise HTTPException(
            status_code=400,
            detail=f"Cannot delete built-in mode '{slug}'"
        )

    unregister_mode(slug)
    return ModeDeleteResponse(deleted=True, slug=slug)


@router.post("/reload", response_model=ModeListResponse)
async def reload_project_modes() -> ModeListResponse:
    """Reload custom modes from .makima/modes.yaml."""
    # Load project modes
    loaded_modes = load_project_modes()

    # Return updated mode list
    all_modes = list_modes()
    return ModeListResponse(
        modes=all_modes,
        total=len(all_modes),
    )