# ============================================================================
# GTFS Validator Web - Production Dockerfile
# Multi-stage build for minimal, secure production image
# ============================================================================

# Stage 1: Build
FROM rust:1.83-bookworm AS builder

WORKDIR /usr/src/app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy the entire workspace
COPY . .

# Build the web service in release mode with optimizations
RUN CARGO_INCREMENTAL=0 cargo build --release --bin gtfs-guru-web

# ============================================================================
# Stage 2: Runtime (minimal image ~100MB)
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies and create non-root user
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -s /bin/false -u 1000 gtfs

WORKDIR /app

# Copy the binary from the builder stage
COPY --from=builder /usr/src/app/target/release/gtfs-guru-web .

# Create data directory with proper permissions
RUN mkdir -p /data/jobs && chown -R gtfs:gtfs /data /app

# Switch to non-root user for security
USER gtfs

# Environment variables with production defaults
ENV GTFS_VALIDATOR_WEB_BASE_DIR=/data/jobs
ENV GTFS_VALIDATOR_WEB_PUBLIC_BASE_URL=http://localhost:3000
ENV GTFS_VALIDATOR_WEB_JOB_TTL_SECONDS=86400
ENV RUST_LOG=info

# Expose the web port
EXPOSE 3000

# Health check for container orchestration
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -sf http://localhost:3000/healthz || exit 1

# Run the validator
CMD ["./gtfs-guru-web"]
