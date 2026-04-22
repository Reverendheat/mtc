# syntax=docker/dockerfile:1.7

FROM rust:1.87-bookworm AS builder
WORKDIR /app

COPY . .

ARG BIN_NAME
RUN test -n "$BIN_NAME"
RUN cargo build --release --bin "$BIN_NAME"

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

ARG BIN_NAME
ARG APP_PORT=8080
ENV APP_PORT=${APP_PORT}

COPY --from=builder /app/target/release/${BIN_NAME} /usr/local/bin/app

EXPOSE ${APP_PORT}

CMD ["/usr/local/bin/app"]
