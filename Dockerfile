# ------------------------------------------------------------------------------
# Build Stage
# ------------------------------------------------------------------------------
FROM rust:1.80-slim-bookworm AS builder

WORKDIR /usr/src/stvault-receipt

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy dependency specifications first for caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src/main.rs and src/lib.rs to cache dependency build
RUN mkdir src && \
    echo "pub fn dummy() {}" > src/lib.rs && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy real source code
COPY src ./src

# Build the release binary
RUN touch src/main.rs src/lib.rs && cargo build --release

# ------------------------------------------------------------------------------
# Runtime Stage
# ------------------------------------------------------------------------------
FROM debian:bookworm-slim

# Install runtime CA certificates for secure HTTPS RPC connections
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -m -u 1000 -U stvault
USER stvault
WORKDIR /app

# Copy binary from builder
COPY --from=builder /usr/src/stvault-receipt/target/release/stvault-receipt /usr/local/bin/stvault-receipt

# Default volume mount for manifests and outputs
VOLUME ["/data"]

ENTRYPOINT ["stvault-receipt"]
CMD ["--help"]
