# Build Stage
FROM rust:1.72.1-slim-bullseye as builder

# Install dependencies required for building
RUN apt-get update && apt-get install -y pkg-config libssl-dev wget

# Copy necessary files and build the application
COPY Cargo.toml ./
RUN mkdir src/
RUN echo "fn main() {println!(\"if you see this, the build broke\")}" > src/main.rs
RUN cargo build --release
RUN rm -f target/release/candle-ocr-server*

COPY src ./src
RUN touch src/main.rs
RUN cargo build --release

# Download and setup ONNX Runtime
RUN wget https://github.com/microsoft/onnxruntime/releases/download/v1.16.3/onnxruntime-linux-x64-1.16.3.tgz
RUN tar -zxvf onnxruntime-linux-x64-1.16.3.tgz
RUN cp -r onnxruntime-linux-x64-1.16.3/include/* /usr/local/include/
RUN cp -r onnxruntime-linux-x64-1.16.3/lib/* /usr/local/lib/
RUN ldconfig

COPY yolov8m.onnx ./
COPY bike.jpeg ./

# Runtime Stage
FROM debian:bullseye-slim

# Copy compiled binary and necessary assets from the builder stage
COPY --from=builder /target/release/yolov8-rs /yolov8-rs
COPY --from=builder /yolov8m.onnx /yolov8m.onnx
COPY --from=builder /bike.jpeg /bike.jpeg
COPY --from=builder /usr/local/lib/* /usr/local/lib/
COPY --from=builder /usr/local/include/* /usr/local/include/

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]

