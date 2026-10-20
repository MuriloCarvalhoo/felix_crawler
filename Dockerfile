FROM rust:1.96-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml clippy.toml ./
COPY crates ./crates
COPY web ./web
RUN cargo build --release --locked -j 2

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends poppler-utils util-linux \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 felix && useradd --uid 10001 --gid 10001 --no-create-home felix \
    && mkdir /data && chown felix:felix /data
COPY --from=build /app/target/release/felix-server /usr/local/bin/felix-server
USER felix
WORKDIR /data
ENV FELIX_BIND=0.0.0.0:8080 DATABASE_PATH=/data/saas_cobrancas.db
EXPOSE 8080
ENTRYPOINT ["felix-server"]
