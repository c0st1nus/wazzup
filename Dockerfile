# Use a multi-stage build to keep the final image small

# Stage 1: Build the application
FROM rust:1.85 AS builder

# Set the working directory
WORKDIR /app

# Copy the Cargo.toml and Cargo.lock files first to cache dependencies
COPY Cargo.toml Cargo.lock ./

# Copy the source code
COPY src ./src

# Build the release binaries
RUN cargo build --release

# Stage 2: Create a slim runtime image
FROM debian:bookworm-slim

# Install any necessary runtime dependencies (e.g., for SQL drivers or TLS)
RUN apt-get update && apt-get install -y \
    libpq5 \
    libmariadb3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /app

# Copy the binaries from the builder stage
COPY --from=builder /app/target/release/main /usr/local/bin/main
COPY --from=builder /app/target/release/admin-cli /usr/local/bin/admin-cli

# Copy static files
COPY static ./static

# Expose the port for the web server (assuming default Actix Web port)
EXPOSE 8080 3245

# Set the default command to run the main web server binary
CMD ["main"]