#!/usr/bin/env bash
# Emit tauri build --bundles args for CI (avoid GitHub 429 on linuxdeploy downloads).
set -euo pipefail

OS="${1:-}"

if [[ "${OS}" == "ubuntu-22.04" ]]; then
  # AppImage pulls linuxdeploy/AppRun from GitHub on every cold runner — parallel
  # release matrix jobs hit 429. .deb is enough for CI; bootstrap falls back to deb.
  printf '%s\n' --bundles deb
fi
