FROM rust:1.85 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p engine-server

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/engine-server /usr/local/bin/
ENV PORT=8080
EXPOSE 8080
CMD ["engine-server"]
