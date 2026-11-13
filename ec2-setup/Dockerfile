# Multi-stage Dockerfile for containerized deployment on AWS EC2 / ECS
FROM rust:1.80-slim as builder

WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-wasip1

COPY . .
RUN cargo build --release -p engine-gateway

# Runtime image
FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/engine-gateway /app/engine-gateway
COPY --from=builder /app/frontend /app/frontend
COPY --from=builder /app/sample-functions /app/sample-functions
RUN mkdir -p /app/target/wasm32-wasip1/release && cp /app/sample-functions/rust_echo.wasm /app/target/wasm32-wasip1/release/

EXPOSE 8080

CMD ["/app/engine-gateway"]
