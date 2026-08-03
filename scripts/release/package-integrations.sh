#!/usr/bin/env bash
# Build versioned companion bundles for GitHub Releases and the release CDN.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
VERSIONS_FILE="${SCRIPT_DIR}/integration-versions.toml"
OUTPUT_DIR="${ROOT}/dist/integrations"

usage() {
  cat <<'EOF'
Usage: scripts/release/package-integrations.sh [--output <dir>]

Requires dependencies to be installed in integrations/vscode, obsidian, and
browser. Builds and packages the four independently versioned companions.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output) OUTPUT_DIR="$2"; shift 2 ;;
    -h | --help) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 1 ;;
  esac
done

if [[ "${OUTPUT_DIR}" != /* ]]; then
  OUTPUT_DIR="${ROOT}/${OUTPUT_DIR}"
fi
mkdir -p "${OUTPUT_DIR}"

integration_version() {
  local id="$1" value
  value="$(sed -n "s/^${id}[[:space:]]*=[[:space:]]*\"\([^\"]*\)\"/\1/p" "${VERSIONS_FILE}" | head -1)"
  [[ -n "${value}" ]] || { echo "error: missing integration version: ${id}" >&2; exit 1; }
  echo "${value}"
}

json_version() {
  node -p "JSON.parse(require('fs').readFileSync(process.argv[1], 'utf8')).version" "$1"
}

assert_version() {
  local id="$1" file="$2" expected actual
  expected="$(integration_version "${id}")"
  actual="$(json_version "${file}")"
  [[ "${actual}" == "${expected}" ]] || {
    echo "error: ${file} version ${actual} does not match ${id} stamp ${expected}" >&2
    exit 1
  }
}

assert_version vscode "${ROOT}/integrations/vscode/package.json"
assert_version vscode "${ROOT}/integrations/vscode/package-lock.json"
assert_version browser "${ROOT}/integrations/browser/package.json"
assert_version browser "${ROOT}/integrations/browser/package-lock.json"
assert_version browser "${ROOT}/integrations/browser/manifest.json"
assert_version obsidian "${ROOT}/integrations/obsidian/package.json"
assert_version obsidian "${ROOT}/integrations/obsidian/package-lock.json"
assert_version obsidian "${ROOT}/integrations/obsidian/manifest.json"

VSCODE_VERSION="$(integration_version vscode)"
NEOVIM_VERSION="$(integration_version neovim)"
BROWSER_VERSION="$(integration_version browser)"
OBSIDIAN_VERSION="$(integration_version obsidian)"

(cd "${ROOT}/integrations/vscode" && npm run package -- --out "${OUTPUT_DIR}/medousa-vscode-v${VSCODE_VERSION}.vsix")
(cd "${ROOT}/integrations/obsidian" && npm run build)
(cd "${ROOT}/integrations/browser" && npm run build)

(cd "${ROOT}/integrations/obsidian" && zip -q -FS -j \
  "${OUTPUT_DIR}/medousa-obsidian-v${OBSIDIAN_VERSION}.zip" \
  main.js manifest.json styles.css)

(cd "${ROOT}/integrations/browser/dist" && zip -q -FS -r \
  "${OUTPUT_DIR}/medousa-browser-v${BROWSER_VERSION}.zip" .)

NEOVIM_STAGE="$(mktemp -d)"
trap 'rm -rf "${NEOVIM_STAGE}"' EXIT
mkdir -p "${NEOVIM_STAGE}/medousa.nvim"
cp -R \
  "${ROOT}/integrations/neovim/README.md" \
  "${ROOT}/integrations/neovim/doc" \
  "${ROOT}/integrations/neovim/lua" \
  "${ROOT}/LICENSE" \
  "${ROOT}/LICENSE-APACHE" \
  "${ROOT}/LICENSE-MIT" \
  "${NEOVIM_STAGE}/medousa.nvim/"
find "${NEOVIM_STAGE}" -name '.DS_Store' -delete
(cd "${NEOVIM_STAGE}" && tar -czf \
  "${OUTPUT_DIR}/medousa-neovim-v${NEOVIM_VERSION}.tar.gz" medousa.nvim)

(
  cd "${OUTPUT_DIR}"
  sha256sum \
    "medousa-vscode-v${VSCODE_VERSION}.vsix" \
    "medousa-neovim-v${NEOVIM_VERSION}.tar.gz" \
    "medousa-browser-v${BROWSER_VERSION}.zip" \
    "medousa-obsidian-v${OBSIDIAN_VERSION}.zip" \
    > INTEGRATIONS-SHA256SUMS
)

echo "Packaged integrations in ${OUTPUT_DIR}:"
ls -la "${OUTPUT_DIR}"
