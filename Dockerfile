# Medousa engine — headless operator image
FROM rust:1.85-bookworm AS builder
WORKDIR /build
COPY . .
RUN cargo build --release -p medousa --bin medousa_daemon --bin medousa --bin medousa_cli

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/medousa /usr/local/bin/medousa
COPY --from=builder /build/target/release/medousa_daemon /usr/local/bin/medousa_daemon
COPY --from=builder /build/target/release/medousa_cli /usr/local/bin/medousa_cli
ENV MEDOUSA_DATA_DIR=/data
VOLUME ["/data"]
EXPOSE 7419
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s \
  CMD curl -fsS http://127.0.0.1:7419/health >/dev/null || exit 1
ENTRYPOINT ["medousa_daemon"]
CMD ["--bind", "0.0.0.0:7419", "--backend", "surreal-mem"]
