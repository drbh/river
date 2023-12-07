# Build Stage
FROM rust:1.72.1-slim-bullseye as builder

# Install dependencies required for building
RUN apt-get update && apt-get install -y pkg-config g++ libssl-dev wget && rm -rf /var/lib/apt/lists/*

# Setting the working directory
WORKDIR /usr/src/river-serve

# Optimizing cargo build cache by copying Cargo.toml and creating a dummy main
COPY river-serve/Cargo.toml .
RUN mkdir src/ && echo "fn main() {println!(\"Dummy Main for caching purposes\")}" > src/main.rs
RUN cargo build --release && rm src/*.rs

# Copying actual source files and building the application
COPY river-serve/src ./src
RUN touch src/main.rs && cargo build --release

FROM rust:1.72.1-slim-bullseye as builder2

# Install dependencies required for building
RUN apt-get update && apt-get install -y pkg-config g++ libssl-dev wget && rm -rf /var/lib/apt/lists/*

# Setting the working directory
WORKDIR /usr/src/river-zmq-proxy

# Optimizing cargo build cache by copying Cargo.toml and creating a dummy main
COPY river-zmq-proxy/Cargo.toml .
RUN mkdir src/ && echo "fn main() {println!(\"Dummy Main for caching purposes\")}" > src/main.rs
RUN cargo build --release && rm src/*.rs

# Copying actual source files and building the application
COPY river-zmq-proxy/src ./src
RUN touch src/main.rs && cargo build --release

# Runtime Stage
FROM debian:bullseye-slim

COPY --from=builder /usr/src/river-serve/target/release/river-serve /usr/local/bin/
COPY --from=builder2 /usr/src/river-zmq-proxy/target/release/river-zmq-proxy /usr/local/bin/

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
