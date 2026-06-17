FROM python:3.12-slim

WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Copy package files
COPY packages/common/pyproject.toml ./packages/common/
COPY packages/schemas/pyproject.toml ./packages/schemas/
COPY apps/backend/pyproject.toml ./apps/backend/

# Copy source code
COPY packages/common/src ./packages/common/src
COPY packages/schemas/src ./packages/schemas/src
COPY apps/backend/src ./apps/backend/src

# Install packages in development mode
RUN pip install --no-cache-dir -e ./packages/common \
    && pip install --no-cache-dir -e ./packages/schemas \
    && pip install --no-cache-dir -e ./apps/backend

# Create non-root user
RUN useradd -m -u 1000 makima && chown -R makima:makima /app
USER makima

# Expose port
EXPOSE 8000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8000/health')" || exit 1

# Run the application
CMD ["python", "-m", "makima.app"]