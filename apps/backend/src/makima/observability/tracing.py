"""OpenTelemetry distributed tracing setup."""

from __future__ import annotations

from typing import Any

from makima_common.config import get_settings
from makima_common.logging import get_logger

logger = get_logger(__name__)


def setup_tracing(app: Any) -> None:
    """Initialize OpenTelemetry tracing for the application."""
    settings = get_settings()

    if not settings.otel_enabled:
        logger.info("OpenTelemetry tracing disabled")
        return

    try:
        from opentelemetry import trace
        from opentelemetry.sdk.trace import TracerProvider
        from opentelemetry.sdk.trace.export import BatchSpanProcessor
        from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
        from opentelemetry.sdk.resources import Resource
        from opentelemetry.instrumentation.fastapi import FastAPIInstrumentor
        from opentelemetry.instrumentation.sqlalchemy import SQLAlchemyInstrumentor

        resource = Resource.create({
            "service.name": settings.otel_service_name,
            "service.version": "0.1.0",
            "deployment.environment": settings.environment,
        })

        provider = TracerProvider(resource=resource)
        exporter = OTLPSpanExporter(endpoint=settings.otel_exporter_endpoint)
        provider.add_span_processor(BatchSpanProcessor(exporter))
        trace.set_tracer_provider(provider)

        FastAPIInstrumentor.instrument_app(app)

        from makima.core.db import engine
        SQLAlchemyInstrumentor().instrument(engine=engine)

        logger.info(
            "OpenTelemetry tracing initialized",
            service=settings.otel_service_name,
            endpoint=settings.otel_exporter_endpoint,
        )

    except ImportError as e:
        logger.warning("OpenTelemetry packages not installed", error=str(e))
    except Exception as e:
        logger.error("Failed to initialize OpenTelemetry", error=str(e))


def get_tracer(name: str) -> Any:
    """Get an OpenTelemetry tracer instance."""
    try:
        from opentelemetry import trace
        return trace.get_tracer(name)
    except ImportError:
        return None