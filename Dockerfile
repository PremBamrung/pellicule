# syntax=docker/dockerfile:1

# ---- build ------------------------------------------------------------------
FROM rust:1-slim-trixie AS build

RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libvips-dev \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

# Cache mounts keep the registry and build artifacts between builds, so
# rebuilds on the NAS only recompile what changed.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked \
 && cp target/release/pellicule /usr/local/bin/pellicule

# ---- runtime ----------------------------------------------------------------
FROM debian:trixie-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends libvips42t64 ffmpeg ca-certificates tini \
 && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 1000 --user-group --no-create-home pellicule \
 && mkdir -p /photos /cache /config \
 && chown pellicule:pellicule /cache

COPY --from=build /usr/local/bin/pellicule /usr/local/bin/pellicule
COPY docker/pellicule.toml /config/pellicule.toml

ENV PELLICULE_CONFIG=/config/pellicule.toml \
    RUST_LOG=pellicule=info

USER pellicule
EXPOSE 8080 8081
VOLUME ["/cache"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["pellicule", "healthcheck"]

ENTRYPOINT ["tini", "--", "pellicule"]
CMD ["serve"]
