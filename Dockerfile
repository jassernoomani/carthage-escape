# syntax=docker/dockerfile:1

# ---- Frontend ----
FROM node:24-slim AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# ---- Server ----
FROM rust:1-slim-bookworm AS server
WORKDIR /server
COPY server/ ./
# Compile-time-checked queries use the committed .sqlx/ cache instead of a live database.
ENV SQLX_OFFLINE=true
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/server/target \
    cargo build --release --locked \
    && cp target/release/carthage-server /carthage-server

# ---- Runtime ----
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 app
WORKDIR /app
COPY --from=server /carthage-server /app/carthage-server
COPY --from=web /web/dist /app/public
COPY content/ /app/content/
USER app
ENV BIND_ADDR=0.0.0.0:8080 STATIC_DIR=/app/public CONTENT_DIR=/app/content
EXPOSE 8080
CMD ["/app/carthage-server"]
