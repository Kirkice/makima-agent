"""FastAPI application entry point."""

from __future__ import annotations

from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from makima import __version__
from makima.routes import admin, audit, auth, health, knowledge, memory, sessions, tasks
from makima.core.middleware import setup_middleware
from makima.observability.metrics import setup_metrics
from makima.observability.tracing import setup_tracing
from makima_common.config import get_settings
from makima_common.logging import setup_logging


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncGenerator[None, None]:
    """Application lifespan — init and shutdown."""
    settings = get_settings()
    setup_logging(debug=settings.debug)

    # Initialize config center
    from makima.config_center.service import config_center
    await config_center.initialize()

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
    app.include_router(audit.router)
    app.include_router(admin.router)

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