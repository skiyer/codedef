# Multi-stage build for minimal image size
FROM rust:1.93-alpine AS builder

# Install musl-dev for static linking
RUN apk add --no-cache musl-dev

WORKDIR /build

# Copy source files
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build static binary
RUN cargo build --release --target x86_64-unknown-linux-musl

# Final minimal image
FROM scratch

# Copy the static binary
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/codedef /codedef

ENTRYPOINT ["/codedef"]
