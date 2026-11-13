#!/usr/bin/env bash
# Offline/synthetic and ephemeral-loopback checks only; no real daemon or providers.
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
export CARGO_TARGET_DIR="$ROOT/target/rndr-content"
OUT="$ROOT/target/rndr-content-evidence"
mkdir -p "$OUT"
MANIFEST="apps/rndr-home/Cargo.toml"
cargo test --manifest-path "$MANIFEST" -j 2 > "$OUT/test-default.log" 2>&1
cargo test --manifest-path "$MANIFEST" --features observe -j 2 > "$OUT/test-observe.log" 2>&1
cargo build --manifest-path "$MANIFEST" --features observe -j 2 > "$OUT/build-observe.log" 2>&1
cargo clippy --manifest-path "$MANIFEST" --all-targets -j 2 -- -D warnings > "$OUT/clippy-default.log" 2>&1
cargo clippy --manifest-path "$MANIFEST" --all-targets --features observe -j 2 -- -D warnings > "$OUT/clippy-observe.log" 2>&1
cargo build --manifest-path "$MANIFEST" -j 2 > "$OUT/build-default.log" 2>&1
cargo fmt --manifest-path "$MANIFEST" --check > "$OUT/fmt.log" 2>&1
bash scripts/verify-docs.sh --strict > "$OUT/docs-strict.log" 2>&1
"$CARGO_TARGET_DIR/debug/medousa-rndr-home" --export-notes "$OUT/renders" > "$OUT/export.log" 2>&1
shasum -a 256 "$CARGO_TARGET_DIR/debug/medousa-rndr-home" > "$OUT/binary.sha256"
printf 'Completed scoped checks and synthetic exports in %s\n' "$OUT"
