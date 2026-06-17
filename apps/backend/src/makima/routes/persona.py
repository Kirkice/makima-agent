"""Persona management API routes."""

from pathlib import Path

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from makima.persona import get_current_persona, set_current_persona, load_persona
from makima_schemas import Persona
from makima_common.logging import get_logger

logger = get_logger(__name__)

router = APIRouter(prefix="/api/persona", tags=["persona"])


class PersonaResponse(BaseModel):
    """Response schema for persona."""

    persona: Persona


class PersonaUpdateRequest(BaseModel):
    """Request schema for updating persona."""

    persona: Persona


@router.get("", response_model=PersonaResponse)
async def get_persona() -> PersonaResponse:
    """Get the current persona configuration."""
    persona = get_current_persona()
    return PersonaResponse(persona=persona)


@router.put("", response_model=PersonaResponse)
async def update_persona(request: PersonaUpdateRequest) -> PersonaResponse:
    """Update the current persona configuration.

    Note: This updates the in-memory persona. To persist changes,
    you need to update the .makima/persona.yaml file.
    """
    set_current_persona(request.persona)
    logger.info("Persona updated via API", name=request.persona.name)
    return PersonaResponse(persona=request.persona)


@router.post("/reload", response_model=PersonaResponse)
async def reload_persona() -> PersonaResponse:
    """Reload persona from .makima/persona.yaml file."""
    project_root = Path.cwd()
    persona = load_persona(project_root)
    set_current_persona(persona)
    logger.info("Persona reloaded from file", name=persona.name)
    return PersonaResponse(persona=persona)


@router.get("/default", response_model=PersonaResponse)
async def get_default_persona() -> PersonaResponse:
    """Get the default built-in persona."""
    from makima.persona.builtin import DEFAULT_PERSONA

    return PersonaResponse(persona=DEFAULT_PERSONA)