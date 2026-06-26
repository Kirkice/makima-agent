"""FastAPI application entry point."""

from __future__ import annotations

from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from sqlalchemy import select

from makima import __version__
from makima.auth.models import User
from makima.auth.service import hash_password, verify_password
from makima.modes import load_all_custom_modes
from makima.routes import admin, attachments, audit, auth, health, knowledge, mcp, memory, modes, model_profiles, persona, sessions, tasks, voice
from makima.core.middleware import setup_middleware
from makima.observability.metrics import setup_metrics
from makima.observability.tracing import setup_tracing
from makima_common.config import get_settings
from makima_common.logging import get_logger, setup_logging


async def ensure_local_cli_user() -> None:
    """Create or update the local desktop login user from env config."""
    settings = get_settings()
    if not settings.cli_username or not settings.cli_password:
        return

    from makima.core.db import async_session_factory

    logger = get_logger(__name__)
    async with async_session_factory() as session:
        result = await session.execute(select(User).where(User.username == settings.cli_username))
        user = result.scalar_one_or_none()

        if user is None:
            user = User(
                username=settings.cli_username,
                email=f"{settings.cli_username}@local.makima",
                hashed_password=hash_password(settings.cli_password),
                role="admin",
            )
            session.add(user)
            await session.commit()
            logger.info("Created local CLI user", username=settings.cli_username)
            return

        if not user.hashed_password or not verify_password(settings.cli_password, user.hashed_password):
            user.hashed_password = hash_password(settings.cli_password)
            await session.commit()
            logger.info("Updated local CLI user password", username=settings.cli_username)


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncGenerator[None, None]:
    """Application lifespan — init and shutdown."""
    settings = get_settings()
    setup_logging(debug=settings.debug)

    # Auto-create database tables (for SQLite development mode)
    from makima.core.db import engine
    from makima.core.models import Base
    import makima.auth.models  # noqa: F401
    import makima.sessions.models  # noqa: F401
    import makima.tasks.models  # noqa: F401
    import makima.audit.models  # noqa: F401
    import makima.knowledge.models  # noqa: F401
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)

    await ensure_local_cli_user()

    # Initialize config center
    from makima.config_center.service import config_center
    await config_center.initialize()

    # Load custom modes from .makima/modes.yaml
    try:
        custom_modes = load_all_custom_modes()
        if custom_modes:
            logger = get_logger(__name__)
            logger.info(f"Loaded {len(custom_modes)} custom modes")
    except Exception as e:
        logger = get_logger(__name__)
        logger.warning(f"Failed to load custom modes: {e}")

    # Cleanup old attachments on startup
    try:
        removed = attachments.cleanup_old_attachments()
        if removed:
            logger = get_logger(__name__)
            logger.info(f"Cleaned up {removed} old attachment session(s)")
    except Exception as e:
        logger = get_logger(__name__)
        logger.warning(f"Failed to cleanup attachments: {e}")

    yield


def create_app() -> FastAPI:
    """Create and configure the FastAPI application."""
    settings = get_settings()
    app = FastAPI(
        title=settings.app_name,
        version=__version__,
        lifespan=lifespan,
    )

    # CORS
    app.add_middleware(
        CORSMiddleware,
        allow_origins=settings.api_cors_origins,
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    # Middleware (RequestID, Timeout)
    setup_middleware(app)

    # Observability
    if settings.prometheus_enabled:
        setup_metrics(app)
    setup_tracing(app)

    # Routes
    app.include_router(health.router)
    app.include_router(auth.router)
    app.include_router(sessions.router)
    app.include_router(tasks.router)
    app.include_router(memory.router)
    app.include_router(knowledge.router)
    app.include_router(modes.router)
    app.include_router(persona.router)
    app.include_router(audit.router)
    app.include_router(admin.router)
    app.include_router(mcp.router)
    app.include_router(voice.router)
    app.include_router(model_profiles.router)
    app.include_router(attachments.router)

    return app


app = create_app()


def main() -> None:
    """Run the application with uvicorn."""
    import uvicorn

    settings = get_settings()
    uvicorn.run(
        "makima.app:app",
        host="0.0.0.0",
        port=8000,
        reload=settings.debug,
    )


if __name__ == "__main__":
    main()
