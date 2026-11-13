# Medousa engine — headless operator image
FROM rust:1.100-bookworm AS builder
WORKDIR /build
COPY . .
# Override the repository's ../.cache/cargo-target so output paths are explicit.
ENV CARGO_TARGET_DIR=/build/target
# Stage standalone artifacts BEFORE enabling tui: Cargo features unify per invocation,
# and a later build may rebuild the daemon as a launcher dependency.
RUN cargo build --locked --release -p medousa --no-default-features --features full-daemon --bin medousa_daemon --bin medousa_cli \
    && mkdir -p /build/packaged \
    && cp /build/target/release/medousa_daemon /build/target/release/medousa_cli /build/packaged/
RUN cargo build --locked --release -p medousa --no-default-features --features tui --bin medousa \
    && cp /build/target/release/medousa /build/packaged/

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/packaged/medousa /usr/local/bin/medousa
COPY --from=builder /build/packaged/medousa_daemon /usr/local/bin/medousa_daemon
COPY --from=builder /build/packaged/medousa_cli /usr/local/bin/medousa_cli
ENV MEDOUSA_DATA_DIR=/data
VOLUME ["/data"]
EXPOSE 7419
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s \
  CMD curl -fsS http://127.0.0.1:7419/health >/dev/null || exit 1
ENTRYPOINT ["medousa_daemon"]
CMD ["--bind", "0.0.0.0:7419", "--backend", "surreal-mem"]
