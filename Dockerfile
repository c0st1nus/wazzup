# Use a multi-stage build to keep the final image small

# Stage 1: Build the application
FROM rust:1.85-slim AS backend-builder

WORKDIR /app

# Install build dependencies (curl is needed for utoipa-swagger-ui build script)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    libmariadb-dev \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy dependency manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy source to cache dependencies
# Copy lib.rs as well to avoid issues with library crates
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn dummy() {}" > src/lib.rs && \
    cargo build --release && \
    rm -rf src target/release/deps/wazzup* target/release/wazzup target/release/wazzup.d

# Copy actual source code
COPY src ./src

# Build the release binary with optimizations
RUN cargo build --release && \
    strip --strip-all target/release/wazzup

# Copy static files
COPY static ./static

# Expose the port for the web server (assuming default Actix Web port)
EXPOSE 8080 3245

# Set the default command to run the main web server binary
CMD ["target/release/wazzup"]