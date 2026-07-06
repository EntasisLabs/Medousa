#!/usr/bin/env bash
# Resolve Tauri bundle output directory (native vs --target, custom CARGO_TARGET_DIR).
set -euo pipefail

CRATE_TAURI="${1:?crate tauri dir required}"
TARGET="${2:-}"
HOST="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p' || true)"

declare -a TARGET_ROOTS=()

append_root() {
  local dir="${1:-}"
  [[ -z "${dir}" ]] && return
  if ((${#TARGET_ROOTS[@]} > 0)); then
    local existing
    for existing in "${TARGET_ROOTS[@]}"; do
      [[ "${existing}" == "${dir}" ]] && return
    done
  fi
  TARGET_ROOTS+=("${dir}")
}

# Env overrides (CI / local).
[[ -n "${CARGO_TARGET_DIR:-}" ]] && append_root "${CARGO_TARGET_DIR}"
[[ -n "${MEDOUSA_CARGO_TARGET_DIR:-}" ]] && append_root "${MEDOUSA_CARGO_TARGET_DIR}"

# Per-crate .cargo/config.toml (medousa-home + medousa-installer use ../.cache/cargo-target).
if [[ -f "${CRATE_TAURI}/.cargo/config.toml" ]]; then
  rel="$(sed -n 's/^target-dir = "\(.*\)"/\1/p' "${CRATE_TAURI}/.cargo/config.toml" | head -1)"
  if [[ -n "${rel}" ]]; then
    append_root "$(cd "${CRATE_TAURI}" && cd "${rel}" && pwd)"
  fi
fi

# Repo default: <parent>/.cache/cargo-target (see .cargo/config.toml at repo root).
repo_root="$(cd "${CRATE_TAURI}/../../.." && pwd)"
append_root "$(cd "${repo_root}/.." && pwd)/.cache/cargo-target"

# Cargo default beside the crate.
append_root "${CRATE_TAURI}/target"

declare -a candidates=()
for root in "${TARGET_ROOTS[@]+"${TARGET_ROOTS[@]}"}"; do
  if [[ -n "${TARGET}" ]]; then
    candidates+=("${root}/${TARGET}/release/bundle")
  fi
  candidates+=("${root}/release/bundle")
  if [[ -n "${HOST}" && "${HOST}" != "${TARGET}" ]]; then
    candidates+=("${root}/${HOST}/release/bundle")
  fi
done

for dir in "${candidates[@]}"; do
  if [[ -d "${dir}" ]]; then
    echo "${dir}"
    exit 0
  fi
done

echo "::error::Tauri bundle directory not found for ${CRATE_TAURI}" >&2
echo "Searched:" >&2
printf '  %s\n' "${candidates[@]}" >&2
for root in "${TARGET_ROOTS[@]+"${TARGET_ROOTS[@]}"}"; do
  if [[ -d "${root}" ]]; then
    echo "layout under ${root}:" >&2
    find "${root}" -maxdepth 4 -type d 2>/dev/null | head -40 >&2 || true
  fi
done
exit 1
