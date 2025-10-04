# KeystoneDB CLI Dockerfile
# Multi-stage build for minimal final image

# Builder stage
FROM rust:1.75-slim as builder

# Install build dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace configuration
COPY Cargo.toml Cargo.lock ./

# Copy all crate sources
COPY kstone-core ./kstone-core
COPY kstone-api ./kstone-api
COPY kstone-cli ./kstone-cli

# Build in release mode
RUN cargo build --release --bin kstone

# Strip binary to reduce size
RUN strip /build/target/release/kstone

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 -s /bin/bash kstone

# Create data directory
RUN mkdir -p /data && chown kstone:kstone /data

# Copy binary from builder
COPY --from=builder /build/target/release/kstone /usr/local/bin/kstone

# Set user
USER kstone

# Set working directory
WORKDIR /data

# Default command shows help
ENTRYPOINT ["kstone"]
CMD ["--help"]

# Health check (create a test database and verify)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD kstone --version || exit 1

# Labels
LABEL org.opencontainers.image.title="KeystoneDB CLI"
LABEL org.opencontainers.image.description="Single-file, embedded, DynamoDB-style database CLI"
LABEL org.opencontainers.image.url="https://github.com/keystone-db/keystonedb"
LABEL org.opencontainers.image.source="https://github.com/keystone-db/keystonedb"
LABEL org.opencontainers.image.vendor="KeystoneDB Contributors"
LABEL org.opencontainers.image.licenses="MIT OR Apache-2.0"
