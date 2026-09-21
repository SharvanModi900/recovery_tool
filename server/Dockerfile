# Build stage
FROM rust:1.76-slim as builder

WORKDIR /app

# Copy core crate
COPY core/ ./core/

# Build the server binary
WORKDIR /app/core
RUN cargo build --release --bin remote_agent_server

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary
COPY --from=builder /app/core/target/release/remote_agent_server /app/remote_agent_server

# Create storage directory
RUN mkdir -p /app/data

# Render sets PORT env variable
ENV PORT=7799
ENV STORAGE_DIR=/app/data

EXPOSE 7799

CMD ["/app/remote_agent_server"]
