# ── Build Stage ──────────────────────────────────────────────────────
FROM rust:1.80-slim AS builder

WORKDIR /app

# Install protoc
RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Copy source
COPY services/tool-runtime/ .

# Build release binary
RUN cargo build --release

# ── Runtime Stage ─────────────────────────────────────────────────────
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/makima-tool-runtime /usr/local/bin/makima-tool-runtime

# Create working directory for tool sandboxing
RUN mkdir -p /tmp/makima-sandbox && chmod 777 /tmp/makima-sandbox

# Expose gRPC port
EXPOSE 50051

ENV RUST_LOG=info

CMD ["makima-tool-runtime"]