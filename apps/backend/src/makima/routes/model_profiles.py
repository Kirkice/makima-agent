"""Model profiles API routes — CRUD for LLM model configurations."""

from __future__ import annotations

import json
from typing import Any

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel, Field

from makima.auth.models import User
from makima.config_center.service import config_center
from makima.core.deps import get_current_user

router = APIRouter(prefix="/api/model", tags=["model-profiles"])

# ─── Schemas ────────────────────────────────────────────────────────────────


class ModelProfile(BaseModel):
    """A single model configuration profile."""

    name: str = Field(..., description="Human-readable profile name")
    provider: str = Field(..., description="Provider type: openai, anthropic, custom, ollama, etc.")
    base_url: str = Field(..., description="API base URL")
    api_key: str | None = Field(None, description="API key (stored encrypted on client)")
    model: str = Field(..., description="Model identifier (e.g. gpt-4o, claude-3-opus)")
    temperature: float = Field(0.7, ge=0.0, le=2.0)
    max_steps: int = Field(100, ge=1, le=5000)
    timeout_seconds: int = Field(300, ge=10, le=3600)
    max_tokens: int | None = Field(None, ge=1)
    thinking_enabled: bool = Field(False)
    thinking_budget: int | None = Field(None, ge=1)


class ModelProfileCreate(BaseModel):
    name: str
    profile: ModelProfile


class ModelProfileUpdate(BaseModel):
    profile: ModelProfile


class ModelConfigStore(BaseModel):
    """Full model configuration store."""

    profiles: dict[str, ModelProfile] = Field(default_factory=dict)
    active_profile: str | None = None


class ModelConfigResponse(BaseModel):
    profiles: dict[str, ModelProfile]
    active_profile: str | None


class ActivateRequest(BaseModel):
    name: str


class TestConnectionRequest(BaseModel):
    base_url: str
    api_key: str | None = None
    model: str


class TestConnectionResponse(BaseModel):
    ok: bool
    message: str
    latency_ms: int | None = None


# ─── Helpers ─────────────────────────────────────────────────────────────────

_PROFILES_KEY = "model.profiles"
_ACTIVE_KEY = "model.active_profile"


async def _load_store() -> ModelConfigStore:
    """Load the model config store from config center."""
    profiles_json = await config_center.get(_PROFILES_KEY)
    active = await config_center.get(_ACTIVE_KEY)

    if profiles_json is None:
        return ModelConfigStore()

    try:
        if isinstance(profiles_json, str):
            profiles_data = json.loads(profiles_json)
        else:
            profiles_data = profiles_json
    except (json.JSONDecodeError, TypeError):
        profiles_data = {}

    profiles: dict[str, ModelProfile] = {}
    for name, pdata in profiles_data.items():
        try:
            profiles[name] = ModelProfile(**pdata)
        except Exception:
            continue

    return ModelConfigStore(profiles=profiles, active_profile=active)


async def _save_store(store: ModelConfigStore) -> None:
    """Persist the model config store to config center."""
    profiles_json = json.dumps(
        {name: p.model_dump() for name, p in store.profiles.items()}
    )
    await config_center.set(_PROFILES_KEY, profiles_json)
    if store.active_profile is not None:
        await config_center.set(_ACTIVE_KEY, store.active_profile)


# ─── Endpoints ───────────────────────────────────────────────────────────────


@router.get("/profiles", response_model=ModelConfigResponse)
async def list_profiles(
    current_user: User = Depends(get_current_user),
) -> ModelConfigResponse:
    """List all model profiles and the active one."""
    store = await _load_store()
    return ModelConfigResponse(
        profiles=store.profiles,
        active_profile=store.active_profile,
    )


@router.post("/profiles", response_model=ModelProfile, status_code=201)
async def create_profile(
    body: ModelProfileCreate,
    current_user: User = Depends(get_current_user),
) -> ModelProfile:
    """Create a new model profile."""
    store = await _load_store()
    if body.name in store.profiles:
        raise HTTPException(status_code=409, detail=f"Profile '{body.name}' already exists")
    store.profiles[body.name] = body.profile
    # Auto-activate if first profile
    if store.active_profile is None and len(store.profiles) == 1:
        store.active_profile = body.name
    await _save_store(store)
    return body.profile


@router.put("/profiles/{name}", response_model=ModelProfile)
async def update_profile(
    name: str,
    body: ModelProfileUpdate,
    current_user: User = Depends(get_current_user),
) -> ModelProfile:
    """Update an existing model profile."""
    store = await _load_store()
    if name not in store.profiles:
        raise HTTPException(status_code=404, detail=f"Profile '{name}' not found")
    store.profiles[name] = body.profile
    await _save_store(store)
    return body.profile


@router.delete("/profiles/{name}")
async def delete_profile(
    name: str,
    current_user: User = Depends(get_current_user),
) -> dict[str, Any]:
    """Delete a model profile."""
    store = await _load_store()
    if name not in store.profiles:
        raise HTTPException(status_code=404, detail=f"Profile '{name}' not found")
    del store.profiles[name]
    if store.active_profile == name:
        store.active_profile = next(iter(store.profiles), None)
    await _save_store(store)
    return {"status": "deleted", "name": name}


@router.post("/profiles/{name}/activate", response_model=ModelConfigResponse)
async def activate_profile(
    name: str,
    current_user: User = Depends(get_current_user),
) -> ModelConfigResponse:
    """Activate a model profile as the current default."""
    store = await _load_store()
    if name not in store.profiles:
        raise HTTPException(status_code=404, detail=f"Profile '{name}' not found")
    store.active_profile = name
    await _save_store(store)
    return ModelConfigResponse(
        profiles=store.profiles,
        active_profile=store.active_profile,
    )


@router.post("/test-connection", response_model=TestConnectionResponse)
async def test_connection(
    body: TestConnectionRequest,
    current_user: User = Depends(get_current_user),
) -> TestConnectionResponse:
    """Test connection to a model provider endpoint."""
    import time
    import httpx

    start = time.monotonic()
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            # Try OpenAI-compatible /models endpoint
            headers = {}
            if body.api_key:
                headers["Authorization"] = f"Bearer {body.api_key}"

            resp = await client.get(
                f"{body.base_url.rstrip('/')}/models",
                headers=headers,
            )
            latency_ms = int((time.monotonic() - start) * 1000)

            if resp.status_code == 200:
                return TestConnectionResponse(
                    ok=True,
                    message="Connection successful",
                    latency_ms=latency_ms,
                )
            elif resp.status_code in (401, 403):
                return TestConnectionResponse(
                    ok=False,
                    message=f"Authentication failed ({resp.status_code})",
                    latency_ms=latency_ms,
                )
            else:
                return TestConnectionResponse(
                    ok=False,
                    message=f"Unexpected status {resp.status_code}",
                    latency_ms=latency_ms,
                )
    except httpx.ConnectError:
        latency_ms = int((time.monotonic() - start) * 1000)
        return TestConnectionResponse(
            ok=False,
            message=f"Cannot connect to {body.base_url}",
            latency_ms=latency_ms,
        )
    except httpx.TimeoutException:
        latency_ms = int((time.monotonic() - start) * 1000)
        return TestConnectionResponse(
            ok=False,
            message="Connection timed out (10s)",
            latency_ms=latency_ms,
        )
    except Exception as e:
        latency_ms = int((time.monotonic() - start) * 1000)
        return TestConnectionResponse(
            ok=False,
            message=f"Error: {e}",
            latency_ms=latency_ms,
        )


@router.get("/providers")
async def list_providers(
    current_user: User = Depends(get_current_user),
) -> list[dict[str, str]]:
    """List supported provider types with default base URLs."""
    return [
        {"name": "openai", "display_name": "OpenAI", "default_base_url": "https://api.openai.com/v1"},
        {"name": "anthropic", "display_name": "Anthropic", "default_base_url": "https://api.anthropic.com"},
        {"name": "deepseek", "display_name": "DeepSeek", "default_base_url": "https://api.deepseek.com/v1"},
        {"name": "gemini", "display_name": "Google Gemini", "default_base_url": "https://generativelanguage.googleapis.com/v1beta"},
        {"name": "ollama", "display_name": "Ollama (local)", "default_base_url": "http://localhost:11434/v1"},
        {"name": "openai-compatible", "display_name": "OpenAI Compatible (custom)", "default_base_url": ""},
        {"name": "mistral", "display_name": "Mistral", "default_base_url": "https://api.mistral.ai/v1"},
        {"name": "openrouter", "display_name": "OpenRouter", "default_base_url": "https://openrouter.ai/api/v1"},
        {"name": "together", "display_name": "Together AI", "default_base_url": "https://api.together.xyz/v1"},
        {"name": "groq", "display_name": "Groq", "default_base_url": "https://api.groq.com/openai/v1"},
    ]