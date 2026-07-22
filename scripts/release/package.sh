#!/usr/bin/env bash
# Deprecated: the full-suite medousa-v* archive is no longer built.
# Use package-component.sh --package engine (or package-all-components.sh).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "warning: package.sh (full-suite medousa-v*) is retired — packaging engine component instead" >&2
exec "${SCRIPT_DIR}/package-component.sh" --package engine "$@"
