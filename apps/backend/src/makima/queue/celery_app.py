"""Celery application configuration."""

from __future__ import annotations

from celery import Celery

from makima_common.config import get_settings


def create_celery_app() -> Celery:
    """Create and configure Celery application."""
    settings = get_settings()

    app = Celery(
        "makima",
        broker=settings.celery_broker_url,
        backend=settings.celery_result_backend,
        include=["makima.queue.tasks"],
    )

    app.conf.update(
        task_serializer="json",
        accept_content=["json"],
        result_serializer="json",
        timezone="UTC",
        enable_utc=True,
        task_acks_late=True,
        task_reject_on_worker_lost=True,
        worker_prefetch_multiplier=1,
        task_default_retry_delay=60,
        task_max_retries=3,
        result_expires=3600,
        task_soft_time_limit=300,
    )

    return app


celery_app = create_celery_app()