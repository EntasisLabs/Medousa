#!/usr/bin/env bash
# Build the Wasmer workshop guest and copy it to crates/medousa-workshop/dist/workshop.wasm.
# Requires cargo-wasix (WASIX sockets). wasm32-unknown-unknown browser artifacts are not this package.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${root}"

if ! command -v cargo-wasix >/dev/null 2>&1; then
  echo "build-workshop-wasm: cargo-wasix is not on PATH" >&2
  echo "install: https://wasix.org/docs/language-guide/rust/installation/" >&2
  exit 1
fi

echo "build-workshop-wasm: cargo wasix build --release -p medousa-workshop"
cargo wasix build --release -p medousa-workshop

if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  target_dir="${CARGO_TARGET_DIR}"
else
  target_dir="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
fi
wasm="${target_dir}/wasm32-wasmer-wasi/release/medousa-workshop.wasm"
if [[ ! -f "${wasm}" ]]; then
  echo "build-workshop-wasm: release wasm not found at ${wasm}" >&2
  exit 1
fi

dest="${root}/crates/medousa-workshop/dist"
mkdir -p "${dest}"
cp "${wasm}" "${dest}/workshop.wasm"

echo "build-workshop-wasm: ${dest}/workshop.wasm"
echo "MEDOUSA_WASMER_PACKAGE=${dest}/workshop.wasm"
echo "MEDOUSA_WASMER_ARGS=--net --no-tty"
