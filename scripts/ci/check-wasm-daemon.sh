#!/usr/bin/env bash
# Compile the browser workshop profile. Native Wasmer, SurrealKV, Forge, and
# Axum stay out of this graph. The cdylib is excluded from the workspace so
# this script builds it with an explicit manifest path.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

target="wasm32-unknown-unknown"
if ! rustup target list --installed | grep -qx "${target}"; then
  echo "check-wasm-daemon: missing Rust target ${target}" >&2
  echo "install it with: rustup target add ${target}" >&2
  exit 1
fi

echo "check-wasm-daemon: medousa wasm-daemon -> ${target}"
cargo check --locked -p medousa \
  --target "${target}" \
  --no-default-features \
  --features wasm-daemon \
  --lib

echo "check-wasm-daemon: page bridge -> ${target}"
cargo check --locked -p medousa-browser --target "${target}"

echo "check-wasm-daemon: OK"
