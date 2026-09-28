#!/usr/bin/env bash
# Build the browser workshop cdylib and, when wasm-bindgen is installed, the
# JS glue Home loads from /wasm/medousa_browser.js.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

target="wasm32-unknown-unknown"
cargo build --locked \
  --manifest-path crates/medousa-browser/Cargo.toml \
  --target "${target}" \
  --release

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "build-wasm-daemon: wasm artifact is ready; install wasm-bindgen-cli to emit JS glue" >&2
  exit 0
fi

out="${repo_root}/apps/medousa-home/static/wasm"
mkdir -p "${out}"
wasm-bindgen \
  --target web \
  --out-dir "${out}" \
  "${CARGO_TARGET_DIR:-target}/${target}/release/medousa_browser.wasm"

echo "build-wasm-daemon: wrote ${out}"
