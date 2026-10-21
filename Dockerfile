# 1) SPA (Vite + React + TanStack)
FROM node:24-bookworm-slim AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend ./
RUN npm run build

# 2) Servidor Rust
FROM rust:1.96-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml clippy.toml ./
COPY crates ./crates
RUN cargo build --release --locked -p felix-server -j 2

# 3) Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends poppler-utils util-linux ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 felix && useradd --uid 10001 --gid 10001 --no-create-home felix
COPY --from=build /app/target/release/felix-server /usr/local/bin/felix-server
COPY --from=frontend /app/frontend/dist /srv/felix/frontend
USER felix
WORKDIR /srv/felix
ENV FELIX_ENV=prod \
    FELIX_BIND=0.0.0.0:8090 \
    FELIX_FRONTEND_DIR=/srv/felix/frontend \
    RUST_LOG=felix_server=info,tower_http=info
EXPOSE 8090
ENTRYPOINT ["felix-server"]
